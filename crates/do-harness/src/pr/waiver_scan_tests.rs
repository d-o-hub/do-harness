//! Unit tests for the text-level waiver scanner.

use super::waiver_scan as scan;

fn lines(source: &str) -> Vec<String> {
    source.lines().map(str::to_owned).collect()
}

#[test]
fn code_only_strips_comments_and_string_bodies() {
    assert_eq!(
        scan::code_only(r#"let x = "a//b"; // tail"#),
        r#"let x = ""; "#
    );
    assert_eq!(scan::code_only("info!(a = 1);"), "info!(a = 1);");
    assert_eq!(scan::code_only("// only a comment"), "");
    assert_eq!(scan::code_only(r#"let p = "a\"b";"#), r#"let p = "";"#);
}

#[test]
fn block_span_matches_braces_and_terminating_semicolons() {
    let source = lines("fn a() {\n    if x {\n    }\n}\n\nmod b;\n");
    assert_eq!(
        scan::block_span(&source, 1),
        scan::Span { start: 1, end: 4 }
    );
    assert_eq!(
        scan::block_span(&source, 6),
        scan::Span { start: 6, end: 6 }
    );
}

#[test]
fn cfg_feature_regions_cover_the_gated_item_only() {
    let source =
        lines("#[cfg(feature = \"csm\")]\npub mod m {\n    pub fn f() {}\n}\n\nfn other() {}\n");
    let regions = scan::cfg_feature_regions(&source);
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].features, vec!["csm".to_owned()]);
    assert_eq!(regions[0].span, scan::Span { start: 1, end: 4 });
    assert!(scan::features_at(&regions, 3).is_some());
    assert!(scan::features_at(&regions, 6).is_none());
}

#[test]
fn cfg_feature_regions_read_multiple_features() {
    let source = lines("#[cfg(all(feature = \"a\", feature = \"b\"))]\nfn g() {}\n");
    let regions = scan::cfg_feature_regions(&source);
    assert_eq!(regions[0].features, vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(scan::join_features(&regions[0].features), "a, b");
}

#[test]
fn macro_spans_cover_only_the_requested_macros() {
    let source = lines(
        "    info!(\n        a = %x.as_str(),\n        \"done\"\n    );\n    other!(\n        b = %y.as_str(),\n    );\n",
    );
    let spans = scan::macro_spans(&source, &["info", "warn"]);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name, "info");
    assert_eq!(spans[0].span, scan::Span { start: 1, end: 4 });
}

#[test]
fn a_non_ascii_line_before_the_macro_does_not_shift_the_span() {
    // The macro's `(` is found as a byte offset; the paren scan counts
    // characters, so a multi-byte character earlier on the line must not shift
    // the span.
    let source = lines(
        "    let café = 1; info!(\n        value = %café.to_string(),\n        \"done\"\n    );\n",
    );
    let spans = scan::macro_spans(&source, &["info"]);
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(spans[0].span, scan::Span { start: 1, end: 4 });
}

#[test]
fn let_else_spans_capture_the_binding_and_body() {
    let source = lines(
        "    for c in candidates {\n        let Some(j) = map.remove(c.id.as_str()) else {\n            return Err(err(c.id));\n        };\n    }\n",
    );
    let spans = scan::let_else_spans(&source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].0, scan::Span { start: 2, end: 4 });
    assert_eq!(spans[0].1, "Some(j) = map.remove(c.id.as_str())");
}

#[test]
fn an_if_else_without_a_let_binding_is_not_a_let_else() {
    let source = lines("    if x {\n    } else {\n        y();\n    }\n");
    assert_eq!(scan::let_else_spans(&source).len(), 0);
}

#[test]
fn match_arms_keep_guards_and_own_their_bodies() {
    let source = lines(
        "fn f(result: R) -> Out {\n    match result {\n        Ok(Some(v)) if v.len() == n => Ok(v),\n        Ok(Some(v)) => {\n            Err(Invalid)\n        }\n        // Unreachable while a judge is configured.\n        Ok(None) => Err(NotConfigured),\n    }\n}\n",
    );
    let arms = scan::match_arms(&source);
    assert_eq!(arms.len(), 3, "{arms:?}");
    assert_eq!(arms[0].head, "Ok(Some(v)) if v.len() == n =>");
    assert_eq!(arms[0].span, scan::Span { start: 3, end: 3 });
    assert_eq!(arms[1].head, "Ok(Some(v)) =>");
    assert_eq!(arms[1].span, scan::Span { start: 4, end: 7 });
    assert_eq!(arms[2].head, "Ok(None) =>");
    assert_eq!(arms[2].span, scan::Span { start: 8, end: 9 });
    assert_eq!(
        scan::unreachable_comment(&source, 8).as_deref(),
        Some("Unreachable while a judge is configured.")
    );
    // The comment is attached to the line it precedes: an arm further up is
    // not covered by it.
    assert_eq!(scan::unreachable_comment(&source, 5), None);
}

#[test]
fn a_body_terminator_starts_the_next_arm() {
    // A block-bodied arm without a following comma must still end the arm.
    let source = lines(
        "    match x {\n        A => {\n            one();\n        }\n        B => two(),\n    }\n",
    );
    let arms = scan::match_arms(&source);
    assert_eq!(arms.len(), 2, "{arms:?}");
    assert_eq!(arms[0].head, "A =>");
    assert_eq!(arms[0].span, scan::Span { start: 2, end: 4 });
    assert_eq!(arms[1].head, "B =>");
}

#[test]
fn unreachable_comments_are_found_above_the_line() {
    let source =
        lines("        // Defensive: unreachable once the judge is configured.\n        Err(x)\n");
    assert!(scan::unreachable_comment(&source, 2).is_some());
    let source = lines("        let y = 1;\n        Err(x)\n");
    assert_eq!(scan::unreachable_comment(&source, 2), None);
}

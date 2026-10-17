//! Marker parsing, EOL handling, and byte-exact splicing.

use crate::docs_generation::markers::{normalize_eol, parse_regions, splice};

#[test]
fn parse_rejects_structural_mistakes() {
    let cases: [(&str, &str); 5] = [
        ("<!-- END GENERATED: x -->\n", "without a BEGIN"),
        ("<!-- BEGIN GENERATED: x -->\n", "without an END"),
        (
            "<!-- BEGIN GENERATED: x -->\n<!-- BEGIN GENERATED: y -->\n",
            "must not nest",
        ),
        (
            "<!-- BEGIN GENERATED: x -->\n<!-- END GENERATED: y -->\n",
            "closed by an END marker for `y`",
        ),
        (
            "<!-- BEGIN GENERATED: x -->\n<!-- END GENERATED: x -->\n\
             <!-- BEGIN GENERATED: x -->\n<!-- END GENERATED: x -->\n",
            "appears more than once",
        ),
    ];
    for (text, expected) in cases {
        let error = parse_regions(text).expect_err("malformed markers must fail");
        assert!(
            error.contains(expected),
            "`{error}` does not mention `{expected}`"
        );
    }
}

#[test]
fn parse_tolerates_trailing_whitespace_and_lists_content_ranges() {
    let text = "lead\n<!-- BEGIN GENERATED: cli-contract pr review -->  \nbody\n<!-- END GENERATED: cli-contract pr review -->\ntail\n";
    let regions = parse_regions(text).expect("markers parse");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].id, "cli-contract pr review");
    assert_eq!(&text[regions[0].content.clone()], "body\n");
}

#[test]
fn splice_replaces_only_the_region() {
    let text = "before\n<!-- BEGIN GENERATED: x -->\nold\n<!-- END GENERATED: x -->\nafter\n";
    let regions = parse_regions(text).expect("markers parse");
    let updated = splice(text, &regions[0], "new\nlines\n");
    assert_eq!(
        updated,
        "before\n<!-- BEGIN GENERATED: x -->\nnew\nlines\n<!-- END GENERATED: x -->\nafter\n"
    );
}

#[test]
fn splice_adds_a_missing_trailing_newline_without_doubling_it() {
    let text = "<!-- BEGIN GENERATED: x -->\nold\n<!-- END GENERATED: x -->\n";
    let regions = parse_regions(text).expect("markers parse");
    assert_eq!(
        splice(text, &regions[0], "one line"),
        "<!-- BEGIN GENERATED: x -->\none line\n<!-- END GENERATED: x -->\n"
    );
    assert_eq!(
        splice(text, &regions[0], "one line\n"),
        "<!-- BEGIN GENERATED: x -->\none line\n<!-- END GENERATED: x -->\n"
    );
}

#[test]
fn crlf_regions_normalize_for_compare_and_keep_crlf_on_write() {
    let text =
        "before\r\n<!-- BEGIN GENERATED: x -->\r\nold\r\n<!-- END GENERATED: x -->\r\nafter\r\n";
    let regions = parse_regions(text).expect("markers parse");
    let region = &regions[0];
    assert_eq!(normalize_eol(&text[region.content.clone()]), "old\n");
    assert_eq!(
        splice(text, region, "old\n"),
        text,
        "matching content is byte-identical"
    );
    assert_eq!(
        splice(text, region, "new\nsecond\n"),
        "before\r\n<!-- BEGIN GENERATED: x -->\r\nnew\r\nsecond\r\n<!-- END GENERATED: x -->\r\nafter\r\n"
    );
}

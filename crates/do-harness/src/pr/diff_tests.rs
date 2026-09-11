//! Parser fixtures for [`super::diff`].

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::diff::{Change, parse};

#[test]
fn parses_modification_hunk_with_anchor() {
    let parsed = parse(
        "diff --git a/src/lib.rs b/src/lib.rs\n\
         index 111..222 100644\n\
         --- a/src/lib.rs\n\
         +++ b/src/lib.rs\n\
         @@ -10,3 +10,4 @@ fn foo() {\n\
         \x20context\n\
         -old\n\
         +new\n\
         +extra\n",
    );
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.units.len(), 1);
    let unit = &parsed.units[0];
    assert_eq!(unit.id, "src/lib.rs:10");
    assert_eq!(unit.path, "src/lib.rs");
    assert_eq!(unit.change, Change::Modified);
    assert_eq!(unit.old_start, 10);
    assert_eq!(unit.new_start, 10);
    assert_eq!(unit.header.as_deref(), Some("fn foo() {"));
    assert_eq!(unit.lines.len(), 4);
}

#[test]
fn new_file_splits_hunks_and_keeps_unique_ids() {
    let parsed = parse(
        "diff --git a/new.rs b/new.rs\n\
         new file mode 100644\n\
         index 000..111\n\
         --- /dev/null\n\
         +++ b/new.rs\n\
         @@ -0,0 +1,2 @@\n\
         +one\n\
         +two\n\
         @@ -5,0 +6,1 @@ fn bar()\n\
         +three\n",
    );
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.units.len(), 2);
    assert_eq!(parsed.units[0].id, "new.rs:1");
    assert_eq!(parsed.units[1].id, "new.rs:6");
    assert!(parsed.units.iter().all(|u| u.change == Change::Added));
    assert_eq!(parsed.units[1].header.as_deref(), Some("fn bar()"));
}

#[test]
fn deletion_is_attributed_to_removed_path() {
    let parsed = parse(
        "diff --git a/gone.rs b/gone.rs\n\
         deleted file mode 100644\n\
         index 111..000\n\
         --- a/gone.rs\n\
         +++ /dev/null\n\
         @@ -1,2 +0,0 @@\n\
         -a\n\
         -b\n",
    );
    assert_eq!(parsed.units.len(), 1);
    assert_eq!(parsed.units[0].path, "gone.rs");
    assert_eq!(parsed.units[0].change, Change::Deleted);
    assert_eq!(parsed.units[0].id, "gone.rs:0");
}

#[test]
fn rename_with_content_is_one_renamed_unit() {
    let parsed = parse(
        "diff --git a/old.rs b/new.rs\n\
         similarity index 80%\n\
         rename from old.rs\n\
         rename to new.rs\n\
         index 111..222 100644\n\
         --- a/old.rs\n\
         +++ b/new.rs\n\
         @@ -1,2 +1,2 @@ fn f()\n\
         \x20keep\n\
         -x\n\
         +y\n",
    );
    assert_eq!(parsed.units.len(), 1);
    assert_eq!(parsed.units[0].path, "new.rs");
    assert_eq!(parsed.units[0].change, Change::Renamed);
}

#[test]
fn rename_only_emits_synthetic_unit() {
    let parsed = parse(
        "diff --git a/old.rs b/new.rs\n\
         similarity index 100%\n\
         rename from old.rs\n\
         rename to new.rs\n",
    );
    assert_eq!(parsed.units.len(), 1);
    assert_eq!(parsed.units[0].path, "new.rs");
    assert_eq!(parsed.units[0].change, Change::Renamed);
    assert_eq!(parsed.units[0].header.as_deref(), Some("rename-only"));
    assert!(parsed.units[0].lines.is_empty());
}

#[test]
fn binary_and_empty_add_emit_units() {
    let parsed = parse(
        "diff --git a/img.png b/img.png\n\
         index 111..222 100644\n\
         Binary files a/img.png and b/img.png differ\n\
         diff --git a/empty.txt b/empty.txt\n\
         new file mode 100644\n\
         index 000..e69\n",
    );
    assert_eq!(parsed.units.len(), 2);
    assert_eq!(parsed.units[0].path, "img.png");
    assert_eq!(parsed.units[0].header.as_deref(), Some("binary"));
    assert_eq!(parsed.units[1].path, "empty.txt");
    assert_eq!(parsed.units[1].change, Change::Added);
    assert_eq!(parsed.units[1].header.as_deref(), Some("empty file"));
}

#[test]
fn mode_change_emits_synthetic_unit() {
    let parsed = parse(
        "diff --git a/run.sh b/run.sh\n\
         old mode 100644\n\
         new mode 100755\n",
    );
    assert_eq!(parsed.units.len(), 1);
    assert_eq!(parsed.units[0].header.as_deref(), Some("mode change"));
    assert_eq!(parsed.units[0].change, Change::Modified);
}

#[test]
fn malformed_header_warns_but_keeps_residual() {
    let parsed = parse(
        "diff --git a/x.rs b/x.rs\n\
         --- a/x.rs\n\
         +++ b/x.rs\n\
         @@ not-a-range @@\n\
         +line\n",
    );
    assert_eq!(parsed.units.len(), 1);
    assert_eq!(parsed.units[0].id, "x.rs:0");
    assert_eq!(parsed.warnings.len(), 1);
}

#[test]
fn quoted_paths_are_decoded() {
    let parsed = parse(
        "diff --git \"a/pa th.rs\" \"b/pa th.rs\"\n\
         index 1..2 100644\n\
         --- \"a/pa th.rs\"\n\
         +++ \"b/pa th.rs\"\n\
         @@ -1 +1 @@\n\
         -a\n\
         +b\n",
    );
    assert_eq!(parsed.units.len(), 1);
    assert_eq!(parsed.units[0].path, "pa th.rs");
}

#[test]
fn duplicate_positions_get_unique_ids() {
    let parsed = parse(
        "diff --git a/x.rs b/x.rs\n\
         --- a/x.rs\n\
         +++ b/x.rs\n\
         @@ -0,0 +0,0 @@\n\
         +a\n\
         @@ -0,0 +0,0 @@\n\
         +b\n",
    );
    assert_eq!(parsed.units.len(), 2);
    assert_ne!(parsed.units[0].id, parsed.units[1].id);
}

//! Unit tests for the patch-coverage waiver classifier.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::waivers::{self, Inputs, Report, WaiverClass};

/// Writes `source` as `src/lib.rs` under a fresh root.
fn root_with(source: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), source).unwrap();
    dir
}

/// A whole-file patch whose added lines are `source`'s lines.
fn patch(path: &str, source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        lines.len()
    );
    for line in lines {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// An lcov report recording every line, with `uncovered` at zero hits.
fn lcov(path: &str, lines: u32, uncovered: &[u32]) -> String {
    let mut out = format!("SF:{path}\n");
    for line in 1..=lines {
        let hits = u32::from(!uncovered.contains(&line));
        let _ = writeln!(out, "DA:{line},{hits}");
    }
    out.push_str("end_of_record\n");
    out
}

/// Classifies `source` with `uncovered` as the measured misses.
fn analyze(source: &str, uncovered: &[u32]) -> (TempDir, Report) {
    let dir = root_with(source);
    let lines = u32::try_from(source.lines().count()).unwrap();
    let report = waivers::analyze(&Inputs {
        patch: &patch("src/lib.rs", source),
        lcov: &lcov("src/lib.rs", lines, uncovered),
        since: None,
        root: dir.path(),
        strip_prefix: None,
    });
    (dir, report)
}

/// The class assigned to one line, when the file is in the report.
fn class_of(report: &Report, path: &str, line: u32) -> Option<WaiverClass> {
    report
        .files
        .iter()
        .find(|file| file.path == path)?
        .lines
        .iter()
        .find(|verdict| verdict.line == line)
        .map(|verdict| verdict.class)
}

#[test]
fn a_logging_field_expression_is_waived_and_a_plain_field_is_not() {
    let source = "\
use tracing::info;

pub fn f(x: usize, y: &str) {
    info!(
        count = x,
        label = %y.trim().to_string(),
        \"done\"
    );
}
";
    let (_dir, report) = analyze(source, &[5, 6]);
    assert_eq!(
        class_of(&report, "src/lib.rs", 6),
        Some(WaiverClass::MacroField)
    );
    assert_eq!(
        class_of(&report, "src/lib.rs", 5),
        Some(WaiverClass::Missing)
    );
    assert_eq!(report.waivable(), 1);
    assert_eq!(report.missing(), 1);
}

#[test]
fn an_arm_after_an_equality_guarded_arm_is_guarded() {
    let source = "\
pub fn f(result: Result<Option<Vec<u32>>, Err>) -> u32 {
    match result {
        Ok(Some(values)) if values.len() == 2 => values.len(),
        Ok(Some(values)) => {
            let count = values.len();
            drop(count);
            0
        }
        Ok(None) => 1,
    }
}
";
    let (_dir, report) = analyze(source, &[4, 5, 6, 7]);
    for line in [4, 5, 6, 7] {
        assert_eq!(
            class_of(&report, "src/lib.rs", line),
            Some(WaiverClass::GuardedArm),
            "line {line}"
        );
    }
    // The equality-guarded arm itself is covered by the report, so it is not
    // classified at all.
    assert_eq!(class_of(&report, "src/lib.rs", 3), None);
}

#[test]
fn a_let_else_fallback_is_guarded() {
    let source = "\
pub fn f(ids: &[String], mut map: std::collections::HashMap<String, u32>) -> usize {
    let mut seen = 0;
    for id in ids {
        let Some(value) = map.remove(id.as_str()) else {
            return Err(JudgmentError::Invalid(format!(
                \"unknown candidate ID: '{}'\",
                id
            )));
        };
        seen += value;
    }
    seen
}
";
    let (_dir, report) = analyze(source, &[5, 6, 7, 8]);
    assert_eq!(
        class_of(&report, "src/lib.rs", 5),
        Some(WaiverClass::GuardedArm)
    );
    assert_eq!(
        class_of(&report, "src/lib.rs", 8),
        Some(WaiverClass::GuardedArm)
    );
    assert!(report.files[0].lines[0].evidence.contains("let … else"));
}

#[test]
fn a_commented_invariant_makes_the_arm_guarded() {
    let source = "\
pub fn f(value: Option<u32>) -> u32 {
    match value {
        Some(v) => v,
        // Unreachable while a judge is configured: never invent a ranking.
        None => 0,
    }
}
";
    let (_dir, report) = analyze(source, &[5]);
    assert_eq!(
        class_of(&report, "src/lib.rs", 5),
        Some(WaiverClass::GuardedArm)
    );
    assert!(report.files[0].lines[0].evidence.contains("Unreachable"));
}

#[test]
fn feature_gated_lines_come_from_the_patch_set_alone() {
    let source = "\
pub mod heuristics;

#[cfg(feature = \"csm\")]
pub mod semantic_rerank {
    pub fn rerank(ids: &[String]) -> Vec<String> {
        ids.to_vec()
    }
}
";
    // The measured pipeline never compiled the gated module: no DA records
    // beyond the ungated line.
    let dir = root_with(source);
    let report = waivers::analyze(&Inputs {
        patch: &patch("src/lib.rs", source),
        lcov: "SF:src/lib.rs\nDA:1,1\nend_of_record\n",
        since: None,
        root: dir.path(),
        strip_prefix: None,
    });
    // The attribute line is part of the gate that excludes everything below it.
    assert_eq!(
        class_of(&report, "src/lib.rs", 3),
        Some(WaiverClass::FeatureGated)
    );
    assert_eq!(
        class_of(&report, "src/lib.rs", 4),
        Some(WaiverClass::FeatureGated)
    );
    assert_eq!(
        class_of(&report, "src/lib.rs", 6),
        Some(WaiverClass::FeatureGated)
    );
    assert_eq!(class_of(&report, "src/lib.rs", 1), None);
    assert!(report.files[0].lines[0].evidence.contains("csm"));
}

#[test]
fn a_feature_gated_line_the_report_covers_is_not_residue() {
    let source = "\
#[cfg(feature = \"csm\")]
pub fn gated() -> u32 {
    1
}
";
    let dir = root_with(source);
    let report = waivers::analyze(&Inputs {
        patch: &patch("src/lib.rs", source),
        // The measured pipeline compiled the feature, so the lines are covered:
        // gating alone is not residue.
        lcov: &lcov("src/lib.rs", 4, &[]),
        since: None,
        root: dir.path(),
        strip_prefix: None,
    });
    assert!(report.files.is_empty(), "{:?}", report.files);
}

#[test]
fn uncovered_lines_outside_the_patch_set_are_ignored() {
    let source = "\
pub fn f() -> u32 {
    let added = 1;
    let covered = 1;
    let missed = 2;
    added + covered + missed
}
";
    // Only line 2 is added; line 4 is context in the same hunk.
    let patch = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,5 @@
 pub fn f() -> u32 {
+    let added = 1;
     let covered = 1;
     let missed = 2;
     added + covered + missed
";
    let dir = root_with(source);
    let report = waivers::analyze(&Inputs {
        patch,
        lcov: &lcov("src/lib.rs", 6, &[4]),
        since: None,
        root: dir.path(),
        strip_prefix: None,
    });
    assert!(report.files.is_empty(), "{:?}", report.files);
    assert_eq!(report.total(), 0);
}

#[test]
fn covered_since_reports_changed_lines_the_previous_report_missed() {
    let source = "\
pub fn f(x: u32) -> u32 {
    let y = x + 1;
    y
}
";
    let dir = root_with(source);
    let report = waivers::analyze(&Inputs {
        patch: &patch("src/lib.rs", source),
        lcov: &lcov("src/lib.rs", 4, &[]),
        since: Some(&lcov("src/lib.rs", 4, &[2, 3])),
        root: dir.path(),
        strip_prefix: None,
    });
    let covered: Vec<u32> = report.covered_since.iter().map(|line| line.line).collect();
    assert_eq!(covered, vec![2, 3]);
    assert_eq!(report.covered_since[0].path, "src/lib.rs");
}

#[test]
fn a_missing_head_source_warns_instead_of_guessing() {
    let dir = tempfile::tempdir().unwrap();
    let report = waivers::analyze(&Inputs {
        patch: &patch("src/gone.rs", "fn f() {}\n"),
        lcov: &lcov("src/gone.rs", 1, &[1]),
        since: None,
        root: dir.path(),
        strip_prefix: None,
    });
    assert!(report.files.is_empty());
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("head source unreadable")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn an_unresolved_previous_report_path_warns() {
    let dir = tempfile::tempdir().unwrap();
    let report = waivers::analyze(&Inputs {
        patch: &patch("src/lib.rs", "fn f() {}\n"),
        lcov: &lcov("src/lib.rs", 1, &[]),
        since: Some(&lcov("/elsewhere/src/lib.rs", 1, &[])),
        root: dir.path(),
        strip_prefix: None,
    });
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("--since path unresolved")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn an_unresolved_lcov_path_is_reported_not_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let report = waivers::analyze(&Inputs {
        patch: &patch("src/lib.rs", "fn f() {}\n"),
        lcov: &lcov("/somewhere/else/src/lib.rs", 1, &[1]),
        since: None,
        root: dir.path(),
        strip_prefix: None,
    });
    assert_eq!(report.unresolved_files, vec!["/somewhere/else/src/lib.rs"]);
}

#[test]
fn the_markdown_comment_carries_classes_and_per_file_counts() {
    let source = "\
use tracing::info;

pub fn f(x: u32, y: &str) {
    info!(
        label = %y.trim().to_string(),
        \"done\"
    );
    let n = x;
    drop(n);
}
";
    let (_dir, report) = analyze(source, &[5, 7]);
    let markdown = waivers::render_markdown(&report);
    assert!(
        markdown.contains("### Patch-coverage residue"),
        "{markdown}"
    );
    assert!(
        markdown.contains("2 uncovered changed line(s)"),
        "{markdown}"
    );
    assert!(markdown.contains("**Waivable by class**"), "{markdown}");
    assert!(markdown.contains("macro-field"), "{markdown}");
    assert!(markdown.contains("**Needs a test**"), "{markdown}");
    assert!(markdown.contains("**Per file**"), "{markdown}");
    assert!(markdown.contains("| `src/lib.rs` | 1 | 1 |"), "{markdown}");
}

#[test]
fn an_empty_residue_renders_a_single_line() {
    let (_dir, report) = analyze("pub fn f() -> u32 {\n    1\n}\n", &[]);
    let markdown = waivers::render_markdown(&report);
    assert!(
        markdown.contains("No uncovered changed lines"),
        "{markdown}"
    );
}

#[test]
fn the_json_report_serializes_classes_and_counts() {
    let source = "use tracing::info;\n\npub fn f(y: &str) {\n    info!(\n        label = %y.trim().to_string(),\n        \"done\"\n    );\n}\n";
    let (_dir, report) = analyze(source, &[5]);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["files"][0]["lines"][0]["class"], "macro-field");
    assert_eq!(json["counts"]["macro-field"], 1);
    assert_eq!(json["counts"].get("missing"), None);
    assert!(Path::new("src/lib.rs").is_relative());
}

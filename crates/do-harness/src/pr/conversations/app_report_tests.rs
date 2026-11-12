//! Unit table for `app-report` classification (#517).
//!
//! The two conditions are tested independently and together, because the whole
//! point of the rule is that neither alone is sufficient: attribution without a
//! failure veto would silence a real failure, and a pass phrase without
//! attribution would silence an arbitrary comment.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use crate::pr::conversations::app_report::{
    AppReport, Attribution, app_slug, check_run_slugs, classify, failure_signal, pass_phrase,
};

fn slugs(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

/// Sonar's real report body on `d-o-hub/rust-self-learning-memory` PR 1263.
const SONAR_PASS: &str = "## [![Quality Gate Passed](https://example.invalid/qg.png 'Quality Gate Passed')] **Quality Gate passed**  \nIssues  \n[0 New issues](https://example.invalid)  \n[0 Accepted issues](https://example.invalid)\n";

/// A benchmark table as `github-actions[bot]` actually posts it.
const BENCHMARK: &str = "## Performance Benchmark Results\n\n<details><summary>Click to expand</summary>\n\n```\ntest cache_basic_cache_hit ... bench: 130.79 ns/iter (+/- 6.17)\ntest cache_eviction ... bench: 339.77 ns/iter (+/- 25.67)\n```\n\n</details>";

#[test]
fn an_attributable_pass_report_is_informational() {
    let head = slugs(&["github-actions", "sonarqubecloud", "codacy-production"]);
    let report = classify("sonarqubecloud[bot]", SONAR_PASS, &head, &BTreeSet::new());
    assert_eq!(report.classification(), "app-report");
    assert!(report.evidence().contains("sonarqubecloud"));
    assert!(report.evidence().contains("check-run author"));
    match report {
        AppReport::Informational {
            slug,
            attribution,
            pass_phrase,
        } => {
            assert_eq!(slug, "sonarqubecloud");
            assert_eq!(attribution, Attribution::CheckRun);
            assert_eq!(pass_phrase.as_deref(), Some("quality gate passed"));
        }
        other @ AppReport::Actionable { .. } => {
            panic!("expected an app report, got {other:?}")
        }
    }
}

/// A benchmark table recognises no pass phrase and is still informational.
///
/// This is the "any other check-run app" case the issue names: the structural
/// attribution decides, so an unrecognised phrasing cannot block a repository.
#[test]
fn an_attributable_report_without_a_recognised_pass_phrase_is_still_informational() {
    let head = slugs(&["github-actions"]);
    let report = classify("github-actions[bot]", BENCHMARK, &head, &BTreeSet::new());
    match report {
        AppReport::Informational {
            slug, pass_phrase, ..
        } => {
            assert_eq!(slug, "github-actions");
            assert_eq!(pass_phrase.as_deref(), Some("benchmark results"));
        }
        other @ AppReport::Actionable { .. } => {
            panic!("expected an app report, got {other:?}")
        }
    }
}

#[test]
fn a_report_with_no_pass_phrase_at_all_is_informational() {
    // Attribution is the decision; phrasing only selects the direction.
    let head = slugs(&["github-actions"]);
    let report = classify(
        "github-actions[bot]",
        "Coverage: 91.2% of lines",
        &head,
        &BTreeSet::new(),
    );
    match report {
        AppReport::Informational { pass_phrase, .. } => assert!(pass_phrase.is_none()),
        other @ AppReport::Actionable { .. } => {
            panic!("expected an app report, got {other:?}")
        }
    }
}

#[test]
fn a_failure_from_an_attributable_app_stays_actionable() {
    let head = slugs(&["sonarqubecloud"]);
    let report = classify(
        "sonarqubecloud[bot]",
        "## **Quality Gate Failed**\n1 New issues",
        &head,
        &BTreeSet::new(),
    );
    assert_eq!(report.classification(), "actionable");
    match report {
        AppReport::Actionable { slug, reason } => {
            assert_eq!(slug.as_deref(), Some("sonarqubecloud"));
            assert!(reason.contains("failure"), "{reason}");
        }
        other @ AppReport::Informational { .. } => {
            panic!("a failing gate must stay actionable, got {other:?}")
        }
    }
}

/// Every failure signal must veto, including one that also reads like a pass.
#[test]
fn the_failure_veto_covers_every_signal() {
    let head = slugs(&["ci-app"]);
    for body in [
        "Quality Gate Passed but 1 test failed",
        "merge blocked by policy",
        "1 vulnerability found",
        "coverage error",
        "critical: 2 issues",
        "the check does not pass",
        "3 checks not pass",
        "unsuccessful run",
        "performance regression detected",
        "breaking change detected",
    ] {
        let report = classify("ci-app[bot]", body, &head, &BTreeSet::new());
        assert!(
            matches!(report, AppReport::Actionable { .. }),
            "{body:?} must stay actionable, got {report:?}"
        );
    }
}

#[test]
fn an_unattributable_bot_asking_a_question_stays_actionable() {
    let head = slugs(&["github-actions"]);
    let report = classify(
        "some-random-app[bot]",
        "Could you clarify why this file changed?",
        &head,
        &BTreeSet::new(),
    );
    match report {
        AppReport::Actionable { slug, reason } => {
            assert_eq!(slug.as_deref(), Some("some-random-app"));
            assert!(reason.contains("no check run"), "{reason}");
        }
        other @ AppReport::Informational { .. } => {
            panic!("an unknown app must stay actionable, got {other:?}")
        }
    }
}

/// A pass phrase does not attribute on its own.
#[test]
fn a_pass_phrase_without_attribution_does_not_qualify() {
    let head = slugs(&["github-actions"]);
    let report = classify(
        "unrelated-bot[bot]",
        "All checks passed per my own reading",
        &head,
        &BTreeSet::new(),
    );
    assert!(matches!(report, AppReport::Actionable { .. }));
}

#[test]
fn a_human_comment_is_not_an_app_report() {
    let head = slugs(&["github-actions"]);
    let report = classify("octocat", SONAR_PASS, &head, &BTreeSet::new());
    match report {
        AppReport::Actionable { slug, reason } => {
            assert!(slug.is_none(), "a human has no app slug");
            assert!(reason.contains("not an attributable app"), "{reason}");
        }
        other @ AppReport::Informational { .. } => {
            panic!("a human must stay actionable, got {other:?}")
        }
    }
}

/// An app whose name merely contains an attributable slug does not inherit it.
#[test]
fn app_matching_is_a_whole_slug_never_a_substring() {
    let head = slugs(&["sonarqubecloud"]);
    for impostor in [
        "not-sonarqubecloud[bot]",
        "sonarqubecloud-helper[bot]",
        "app/sonarqubecloud2[bot]",
    ] {
        let report = classify(impostor, SONAR_PASS, &head, &BTreeSet::new());
        assert!(
            matches!(report, AppReport::Actionable { .. }),
            "{impostor} must not borrow the slug"
        );
    }
}

#[test]
fn the_allowlist_attributes_an_app_with_no_check_run_on_this_head() {
    // A reused pull request keeps the classification it had before the head
    // moved, which is what the override is for.
    let report = classify(
        "sonarqubecloud[bot]",
        SONAR_PASS,
        &BTreeSet::new(),
        &slugs(&["sonarqubecloud"]),
    );
    match report {
        AppReport::Informational { attribution, .. } => {
            assert_eq!(attribution, Attribution::Allowlist);
        }
        other @ AppReport::Actionable { .. } => {
            panic!("expected the allowlist to attribute, got {other:?}")
        }
    }
}

/// A review app's comment is a review, even when it is green and attributable.
#[test]
fn a_representative_review_app_is_never_an_app_report() {
    let head = slugs(&["coderabbitai", "sourcery-ai"]);
    for login in ["coderabbitai[bot]", "sourcery-ai[bot]"] {
        let report = classify(
            login,
            "All checks passed in my review",
            &head,
            &BTreeSet::new(),
        );
        match report {
            AppReport::Actionable { reason, .. } => {
                assert!(reason.contains("code-review app"), "{reason}");
            }
            other @ AppReport::Informational { .. } => {
                panic!("{login} must stay actionable, got {other:?}")
            }
        }
    }
}

#[test]
fn app_slug_only_accepts_the_app_form() {
    assert_eq!(
        app_slug("sonarqubecloud[bot]").as_deref(),
        Some("sonarqubecloud")
    );
    assert_eq!(app_slug("app/dependabot").as_deref(), None);
    assert_eq!(app_slug("octocat"), None);
    assert_eq!(app_slug("[bot]"), None);
    assert_eq!(app_slug("  ci[bot]  ").as_deref(), Some("ci"));
}

#[test]
fn legacy_status_sources_are_not_apps() {
    let head = check_run_slugs(["status", "github-actions", ""]);
    assert!(!head.contains("status"));
    assert!(!head.contains(""));
    assert!(head.contains("github-actions"));
}

#[test]
fn failure_signal_and_pass_phrase_are_independent_helpers() {
    assert_eq!(failure_signal("all good"), None);
    assert!(failure_signal("1 Failed").is_some());
    assert_eq!(pass_phrase("nothing special"), None);
    assert!(pass_phrase("Quality Gate Passed").is_some());
}

//! Unit tests for PR merge-readiness analysis (`readiness.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::pr::gh;

fn mock_view(number: u64, merge_state: &str) -> gh::PrViewExtended {
    gh::PrViewExtended {
        number,
        title: "Test PR".to_string(),
        url: format!("https://github.com/example/repo/pull/{number}"),
        state: "OPEN".to_string(),
        mergeable: true,
        merge_state_status: merge_state.to_string(),
        base_ref_name: "main".to_string(),
        head_ref_oid: "abc123def456".to_string(),
    }
}

#[test]
fn clean_pr_with_all_passing_checks_is_ready_to_merge() {
    let view = mock_view(100, "CLEAN");
    let check_runs = vec![
        gh::CheckRunItem {
            name: "test".to_string(),
            status: "completed".to_string(),
            conclusion: Some("success".to_string()),
            html_url: Some("https://github.com/example/repo/actions/runs/111".to_string()),
            details_url: None,
        },
        gh::CheckRunItem {
            name: "lint".to_string(),
            status: "completed".to_string(),
            conclusion: Some("success".to_string()),
            html_url: Some("https://github.com/example/repo/actions/runs/112".to_string()),
            details_url: None,
        },
    ];

    let report = analyze_readiness_data(&view, &check_runs, &[], &[], &[], &[], &[]);
    assert!(report.ready_to_merge);
    assert_eq!(report.checks.passed, 2);
    assert_eq!(report.checks.cancelled, 0);
    assert_eq!(report.checks.failed, 0);
    assert!(report.actionable_items.is_empty());
}

#[test]
fn pr_with_cancelled_check_legs_exits_non_zero_and_provides_rerun_command() {
    // Evidence 1 scenario: Storage Matrix legs cancelled
    let view = mock_view(1041, "CLEAN");
    let check_runs = vec![
        gh::CheckRunItem {
            name: "Storage Matrix (Ubuntu)".to_string(),
            status: "completed".to_string(),
            conclusion: Some("cancelled".to_string()),
            html_url: Some("https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635041/job/1".to_string()),
            details_url: None,
        },
        gh::CheckRunItem {
            name: "Storage Matrix (macOS)".to_string(),
            status: "completed".to_string(),
            conclusion: Some("cancelled".to_string()),
            html_url: Some("https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635042/job/2".to_string()),
            details_url: None,
        },
        gh::CheckRunItem {
            name: "Storage Matrix (Windows)".to_string(),
            status: "completed".to_string(),
            conclusion: Some("cancelled".to_string()),
            html_url: Some("https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635043/job/3".to_string()),
            details_url: None,
        },
    ];

    let report = analyze_readiness_data(&view, &check_runs, &[], &[], &[], &[], &[]);
    assert!(!report.ready_to_merge);
    assert_eq!(report.checks.cancelled, 3);
    assert_eq!(report.actionable_items.len(), 3);

    for item in &report.actionable_items {
        assert_eq!(item.kind, "cancelled_check");
        assert!(item.title.contains("Storage Matrix"));
        assert!(item.rerun_command.is_some());
        let cmd = item.rerun_command.as_ref().unwrap();
        assert!(cmd.starts_with("gh run rerun "));
        assert!(cmd.ends_with(" --failed"));
    }
}

#[test]
fn pr_with_unanswered_codecov_gap_lists_comment_as_actionable() {
    // Evidence 2 scenario: Codecov comment reporting 0% patch coverage
    let view = mock_view(1042, "CLEAN");
    let issue_comments = vec![gh::IssueCommentItem {
        id: 1,
        user: gh::UserLogin {
            login: "codecov[bot]".to_string(),
        },
        body: "## Codecov Report\n\n> Patch Coverage: 0.00% of diff\n\nMissing Lines in CLI entry point".to_string(),
        html_url: Some("https://github.com/example/repo/pull/1042#issuecomment-1".to_string()),
    }];

    let report = analyze_readiness_data(&view, &[], &[], &[], &[], &issue_comments, &[]);
    assert!(!report.ready_to_merge);
    assert!(report.codecov.is_some());

    let codecov = report.codecov.as_ref().unwrap();
    assert_eq!(codecov.status, "actionable_gap");
    assert_eq!(codecov.patch_coverage.as_deref(), Some("0.00%"));

    let gap_item = report
        .actionable_items
        .iter()
        .find(|item| item.kind == "codecov_gap")
        .expect("codecov_gap actionable item");
    assert!(gap_item.description.contains("0.00%"));
}

#[test]
fn non_clean_merge_state_blocks_merge() {
    let view = mock_view(1050, "BEHIND");
    let report = analyze_readiness_data(&view, &[], &[], &[], &[], &[], &[]);
    assert!(!report.ready_to_merge);

    let state_item = report
        .actionable_items
        .iter()
        .find(|item| item.kind == "merge_state")
        .expect("merge_state actionable item");
    assert!(state_item.title.contains("BEHIND"));
}

#[test]
fn unresolved_review_thread_blocks_merge() {
    let view = mock_view(1060, "CLEAN");
    let threads = vec![gh::GqlThreadItem {
        id: "thread1".to_string(),
        is_resolved: false,
        path: "src/main.rs".to_string(),
        line: 42,
        author: "reviewer".to_string(),
    }];

    let report = analyze_readiness_data(&view, &[], &[], &[], &[], &[], &threads);
    assert!(!report.ready_to_merge);
    assert_eq!(report.comments.review_threads_unresolved, 1);

    let thread_item = report
        .actionable_items
        .iter()
        .find(|item| item.kind == "unresolved_thread")
        .expect("unresolved_thread actionable item");
    assert!(thread_item.description.contains("src/main.rs:42"));
}

#[test]
fn extract_rerun_command_parses_run_id() {
    let url1 = "https://github.com/d-o-hub/rust-self-learning-memory/actions/runs/35885635041/job/99123456";
    assert_eq!(
        extract_rerun_command(url1),
        Some("gh run rerun 35885635041 --failed".to_string())
    );

    let url2 = "https://example.com/other/path";
    assert_eq!(extract_rerun_command(url2), None);
}

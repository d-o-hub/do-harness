//! Agent-mode eval tests (`eval --agent-cmd`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

use anyhow::Result;

use super::agent::{AgentSpec, run_agent, run_agent_with_redactor};
use super::eval_tests::{VALID_SKILL_MD, fixture_root};
use super::orchestrator::run_eval;
use crate::report::Format;

async fn eval_run_agent(dir: &Path, agent_cmd: &str, timeout_secs: u64) -> Result<()> {
    run_eval(
        dir,
        super::EvalOpts {
            skill: None,
            bless: false,
            list_skills: false,
            fail_fast: false,
            dry_run: false,
            format: Format::Text,
            approver: Some("test-approver"),
            no_lift: false,
            agent_cmd: Some(agent_cmd),
            agent_timeout_secs: timeout_secs,
            strict_fixtures: false,
            trials: 1,
        },
    )
    .await
}

fn agent_case_json(assertions: &[&str]) -> String {
    let list: Vec<String> = assertions.iter().map(|a| format!("\"{a}\"")).collect();
    format!(
        r#"{{
          "skill_name": "test-skill",
          "evals": [
            {{
              "id": 1,
              "prompt": "guided task",
              "expected_output": "guided out",
              "files": [],
              "dim": "effectiveness",
              "assertions": [{}]
            }}
          ]
        }}"#,
        list.join(", ")
    )
}

const GUIDED_STUB: &str = "if [ -f .agents/skills/test-skill/SKILL.md ]; then \
     printf 'with-guidance' > answer.txt; else printf 'no-guidance' > answer.txt; fi";

#[tokio::test(flavor = "current_thread")]
async fn agent_mode_measures_real_lift_and_persists_mode() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&agent_case_json(&[
            "exists:answer.txt",
            "contains:answer.txt|with-guidance",
        ])),
    );

    eval_run_agent(dir.path(), GUIDED_STUB, 60).await.unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let run = do_harness_db::latest_eval_run(&conn, "test-skill")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.mode, do_harness_types::EvalMode::Agent);
    assert_eq!(run.pass_rate, Some(1.0));
    assert_eq!(run.without_pass_rate, Some(0.5));
    let rates = do_harness_db::dim_rates_for_run(&conn, run.id)
        .await
        .unwrap();
    assert_eq!(rates.len(), 1);
    assert_eq!(rates[0].dim, "effectiveness");
    assert_eq!(rates[0].graded, 2);
    assert_eq!(rates[0].passed, 2);
    assert_eq!(rates[0].without_passed, Some(1));
}

#[tokio::test(flavor = "current_thread")]
async fn agent_stdout_is_gradable() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&agent_case_json(&[
            "contains:agent_stdout.txt|verdict: pass",
        ])),
    );

    eval_run_agent(dir.path(), "printf 'verdict: pass\\n'", 60)
        .await
        .unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let run = do_harness_db::latest_eval_run(&conn, "test-skill")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.pass_rate, Some(1.0));
}

#[tokio::test(flavor = "current_thread")]
async fn agent_failure_fails_all_graded_assertions() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&agent_case_json(&["exists:."])));

    eval_run_agent(dir.path(), "echo broken >&2; exit 9", 60)
        .await
        .unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let run = do_harness_db::latest_eval_run(&conn, "test-skill")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.pass_rate, Some(0.0));
}

#[tokio::test(flavor = "current_thread")]
async fn agent_timeout_fails_the_run() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&agent_case_json(&["exists:."])));

    eval_run_agent(dir.path(), "sleep 30", 1).await.unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let run = do_harness_db::latest_eval_run(&conn, "test-skill")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.pass_rate, Some(0.0));
    assert_eq!(run.mode, do_harness_types::EvalMode::Agent);
}

async fn eval_run_agent_bless(dir: &Path, agent_cmd: &str, timeout_secs: u64) -> Result<()> {
    run_eval(
        dir,
        super::EvalOpts {
            skill: None,
            bless: true,
            list_skills: false,
            fail_fast: false,
            dry_run: false,
            format: Format::Text,
            approver: Some("test-approver"),
            no_lift: false,
            agent_cmd: Some(agent_cmd),
            agent_timeout_secs: timeout_secs,
            strict_fixtures: false,
            trials: 1,
        },
    )
    .await
}

/// Agent runs are judged by agent floors only: a deterministic floor must not
/// fail them, blessing ratchets the agent floor, and that floor then governs.
#[tokio::test(flavor = "current_thread")]
async fn agent_runs_use_agent_floors_only() {
    use do_harness_types::EvalMode;

    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&agent_case_json(&[
            "exists:answer.txt",
            "contains:answer.txt|with-guidance",
        ])),
    );
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    do_harness_db::raise_lift_floor(&conn, "test-skill", EvalMode::Deterministic, 0.9)
        .await
        .unwrap();
    do_harness_db::raise_skill_bar(&conn, "test-skill", EvalMode::Deterministic, 0.99)
        .await
        .unwrap();
    drop(conn);

    // Deterministic floors do not apply: agent lift is 0.5, pass rate 1.0.
    eval_run_agent(dir.path(), GUIDED_STUB, 60).await.unwrap();

    eval_run_agent_bless(dir.path(), GUIDED_STUB, 60)
        .await
        .unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    assert_eq!(
        do_harness_db::get_lift_floor(&conn, "test-skill", EvalMode::Agent)
            .await
            .unwrap(),
        Some(0.45)
    );
    assert_eq!(
        do_harness_db::get_lift_floor(&conn, "test-skill", EvalMode::Deterministic)
            .await
            .unwrap(),
        Some(0.9)
    );
    do_harness_db::raise_lift_floor(&conn, "test-skill", EvalMode::Agent, 0.6)
        .await
        .unwrap();
    drop(conn);

    let err = eval_run_agent(dir.path(), GUIDED_STUB, 60)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("test-skill"), "{err:#}");
}

#[cfg(unix)]
const DESCENDANT_SLEEP_SECS: u64 = 4;
#[cfg(unix)]
const DESCENDANT_COMPLETION_LIMIT: Duration = Duration::from_secs(3);

#[test]
fn agent_output_over_budget_fails_and_retains_bounded_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let limit = crate::config::DEFAULT_OUTPUT_BYTES;
    let spec = AgentSpec {
        command: format!("printf '%{}s' ''", limit + 1),
        timeout: Duration::from_secs(5),
    };
    let run = run_agent(&spec, "prompt", dir.path());
    assert!(!run.success, "oversized output passed");
    let detail = run.detail.expect("output-limit failure detail");
    assert!(detail.contains("output limit"), "detail: {detail}");
    let stdout = std::fs::read(dir.path().join("agent_stdout.txt")).unwrap();
    assert!(
        stdout.len() <= limit,
        "retained {} bytes, limit {limit}",
        stdout.len()
    );
}

#[cfg(unix)]
#[test]
fn agent_timeout_kills_descendants_that_retain_output_pipes() {
    let dir = tempfile::tempdir().unwrap();
    let spec = AgentSpec {
        command: format!("sleep {DESCENDANT_SLEEP_SECS} & wait"),
        timeout: Duration::from_millis(100),
    };
    let started = Instant::now();
    let run = run_agent(&spec, "prompt", dir.path());
    assert!(!run.success);
    assert!(
        started.elapsed() < DESCENDANT_COMPLETION_LIMIT,
        "inherited pipe held completion for {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn agent_successful_parent_kills_pipe_holding_descendants() {
    let dir = tempfile::tempdir().unwrap();
    let spec = AgentSpec {
        command: format!("sleep {DESCENDANT_SLEEP_SECS} &"),
        timeout: Duration::from_secs(DESCENDANT_SLEEP_SECS + 1),
    };
    let started = Instant::now();
    let run = run_agent(&spec, "prompt", dir.path());
    assert!(run.success, "detail: {:?}", run.detail);
    assert!(
        started.elapsed() < DESCENDANT_COMPLETION_LIMIT,
        "inherited pipe held completion for {:?}",
        started.elapsed()
    );
}

#[test]
fn agent_redacts_synthetic_secrets_before_retention() {
    let dir = tempfile::tempdir().unwrap();
    let secret = "synthetic-agent-private";
    let spec = AgentSpec {
        command: format!("printf '%s' '{secret}'; printf '%s' '{secret}' >&2; exit 1"),
        timeout: Duration::from_secs(5),
    };
    let redactor = crate::redaction::Redactor::synthetic(&[secret]);
    let run = run_agent_with_redactor(&spec, "prompt", dir.path(), redactor);
    let detail = run.detail.expect("agent failure detail");
    assert!(
        !detail.contains(secret),
        "secret leaked in detail: {detail}"
    );
    assert!(detail.contains("[REDACTED]"), "detail: {detail}");
    let stdout = std::fs::read_to_string(dir.path().join("agent_stdout.txt")).unwrap();
    assert!(
        !stdout.contains(secret),
        "secret leaked in stdout: {stdout}"
    );
    assert!(stdout.contains("[REDACTED]"), "stdout: {stdout}");
}

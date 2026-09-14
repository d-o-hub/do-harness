//! Agent-mode eval tests (`eval --agent-cmd`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use anyhow::Result;

use super::eval_tests::{VALID_SKILL_MD, fixture_root};
use super::orchestrator::run_eval;
use crate::report::Format;

async fn eval_run_agent(dir: &Path, agent_cmd: &str, timeout_secs: u64) -> Result<()> {
    run_eval(
        dir,
        None,
        false,
        false,
        false,
        false,
        Format::Text,
        Some("test-approver"),
        false,
        Some(agent_cmd),
        timeout_secs,
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

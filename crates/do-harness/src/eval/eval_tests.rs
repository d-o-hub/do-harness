//! Unit tests for the `do-harness eval` runner (`eval.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const VALID_SKILL_MD: &str = "---\nname: test-skill\ndescription: A fixture skill used by the eval-runner tests.\nlicense: MIT\n---\n\n# Test Skill\n";

fn gate_script_path() -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    repo_root.join(".agents/skills/skill-creator/scripts/quick_validate.py")
}

fn fixture_root(skill_md: &str, evals: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let skills_root = dir.path().join(".agents/skills");
    let scripts = skills_root.join("skill-creator/scripts");
    fs::create_dir_all(&scripts).unwrap();
    fs::copy(gate_script_path(), scripts.join("quick_validate.py")).unwrap();
    let skill_dir = skills_root.join("test-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();
    if let Some(json) = evals {
        fs::create_dir_all(skill_dir.join("evals")).unwrap();
        fs::write(skill_dir.join("evals/evals.json"), json).unwrap();
    }
    dir
}

async fn persisted(dir: &Path) -> Vec<do_harness_types::SkillEval> {
    let conn = do_harness_db::connect_and_migrate(dir).await.unwrap();
    do_harness_db::list_skill_evals(&conn, "test-skill")
        .await
        .unwrap()
}

fn single_case_json(assertions: &[&str]) -> String {
    let list: Vec<String> = assertions.iter().map(|a| format!("\"{a}\"")).collect();
    format!(
        r#"{{
          "skill_name": "test-skill",
          "evals": [
            {{
              "id": 1,
              "prompt": "prompt one",
              "expected_output": "out one",
              "files": [],
              "assertions": [{}]
            }}
          ]
        }}"#,
        list.join(", ")
    )
}

async fn eval_run(dir: &Path, skill: Option<&str>, bless: bool) -> Result<()> {
    run_eval(dir, skill, bless, false, false, false, Format::Text).await
}

#[tokio::test(flavor = "current_thread")]
async fn exists_assertion_passes_and_persists_rich_data() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&single_case_json(&[
            "exists:.agents/skills/test-skill/artifact.md",
        ])),
    );
    fs::write(
        dir.path().join(".agents/skills/test-skill/artifact.md"),
        "# artifact\n",
    )
    .unwrap();

    eval_run(dir.path(), None, false).await.unwrap();
    let rows = persisted(dir.path()).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].prompt.as_deref(), Some("prompt one"));
    assert_eq!(rows[0].expected_outcome.as_deref(), Some("out one"));
    assert_eq!(rows[0].pass_rate, Some(1.0));
}

#[tokio::test(flavor = "current_thread")]
async fn bless_then_drift_then_bar_miss() {
    let dir = fixture_root(VALID_SKILL_MD, Some(&single_case_json(&["exists:."])));
    let evals_path = dir
        .path()
        .join(".agents/skills/test-skill/evals/evals.json");

    eval_run(dir.path(), None, true).await.unwrap();
    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    assert!(
        do_harness_db::get_grader_baseline(&conn, "test-skill")
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        do_harness_db::get_skill_bar(&conn, "test-skill")
            .await
            .unwrap(),
        Some(0.95)
    );
    drop(conn);

    fs::write(
        &evals_path,
        fs::read_to_string(&evals_path).unwrap().replace('1', "9"),
    )
    .unwrap();
    let err = eval_run(dir.path(), None, false).await.unwrap_err();
    assert!(err.to_string().contains("test-skill"), "{err:#}");

    fs::write(&evals_path, single_case_json(&["exists:."])).unwrap();
    eval_run(dir.path(), None, true).await.unwrap();
    let degraded = single_case_json(&[
        "exists:.",
        "contains:.agents/skills/test-skill/SKILL.md|absent-needle",
    ]);
    fs::write(&evals_path, degraded).unwrap();
    let err = eval_run(dir.path(), None, false).await.unwrap_err();
    assert!(err.to_string().contains("test-skill"), "{err:#}");
}

#[tokio::test(flavor = "current_thread")]
async fn contains_assertion_passes() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&single_case_json(&[
            "exists:.agents/skills/test-skill/report.md",
            "contains:.agents/skills/test-skill/report.md|Graded by assertions",
        ])),
    );
    fs::write(
        dir.path().join(".agents/skills/test-skill/report.md"),
        "Summary\nGraded by assertions, not JSON counting.\n",
    )
    .unwrap();

    eval_run(dir.path(), None, false).await.unwrap();
    let rows = persisted(dir.path()).await;
    assert_eq!(rows[0].pass_rate, Some(1.0));
}

#[tokio::test(flavor = "current_thread")]
async fn failing_db_assertion_drives_pass_rate_to_zero() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&single_case_json(&[
            "db:heuristics:skill_name=ghost-skill:min=1",
        ])),
    );

    eval_run(dir.path(), Some("test-skill"), false)
        .await
        .unwrap();
    let rows = persisted(dir.path()).await;
    assert_eq!(rows[0].pass_rate, Some(0.0));
}

#[tokio::test(flavor = "current_thread")]
async fn unprefixed_assertions_are_documentation_excluded_from_pass_rate() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&single_case_json(&[
            "exists:.agents/skills/test-skill/readme.md",
            "Uses a typed Command struct",
            "Follows the self-correction protocol",
        ])),
    );
    fs::write(
        dir.path().join(".agents/skills/test-skill/readme.md"),
        "readme body\n",
    )
    .unwrap();

    eval_run(dir.path(), None, false).await.unwrap();
    let rows = persisted(dir.path()).await;
    assert_eq!(rows[0].pass_rate, Some(1.0));
}

#[tokio::test(flavor = "current_thread")]
async fn walkthrough_artifact_then_exists_passes() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&single_case_json(&[
            "exists:generated/summary.txt",
            "contains:generated/summary.txt|generated-by-walkthrough",
        ])),
    );
    fs::create_dir_all(dir.path().join(".agents/skills/test-skill/evals")).unwrap();
    let script = dir
        .path()
        .join(".agents/skills/test-skill/evals/walkthrough.sh");
    fs::write(
        &script,
        "#!/bin/sh\nmkdir -p \"$DO_HARNESS_ROOT/generated\"\n\
         echo \"generated-by-walkthrough\" > \"$DO_HARNESS_ROOT/generated/summary.txt\"\n\
         echo \"$DO_HARNESS_ROOT\" >> \"$DO_HARNESS_ROOT/generated/summary.txt\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    eval_run(dir.path(), None, false).await.unwrap();
    let rows = persisted(dir.path()).await;
    assert_eq!(rows[0].pass_rate, Some(1.0));
}

#[tokio::test(flavor = "current_thread")]
async fn failing_walkthrough_fails_all_graded_assertions() {
    let dir = fixture_root(
        VALID_SKILL_MD,
        Some(&single_case_json(&["exists:something.txt"])),
    );
    let script = dir
        .path()
        .join(".agents/skills/test-skill/evals/walkthrough.sh");
    fs::write(&script, "#!/bin/sh\nexit 7\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    eval_run(dir.path(), None, false).await.unwrap();
    let rows = persisted(dir.path()).await;
    assert_eq!(rows[0].pass_rate, Some(0.0));
}

#[tokio::test(flavor = "current_thread")]
async fn skill_without_evals_skips_persistence() {
    let dir = fixture_root(VALID_SKILL_MD, None);
    eval_run(dir.path(), None, false).await.unwrap();
    assert!(persisted(dir.path()).await.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn skill_failing_structure_gate_errors() {
    let md = "# No frontmatter here\n";
    let dir = fixture_root(md, None);
    let err = eval_run(dir.path(), None, false).await.unwrap_err();
    assert!(err.to_string().contains("test-skill"));
}

#[tokio::test(flavor = "current_thread")]
async fn missing_gate_script_is_not_a_structure_failure() {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join(".agents/skills/alpha");
    fs::create_dir_all(skill_dir.join("evals")).unwrap();
    fs::write(skill_dir.join("SKILL.md"), VALID_SKILL_MD).unwrap();
    fs::write(
        skill_dir.join("evals/evals.json"),
        single_case_json(&["exists:.agents/skills/alpha/alpha.txt"]),
    )
    .unwrap();
    fs::write(skill_dir.join("alpha.txt"), "x").unwrap();

    eval_run(dir.path(), Some("alpha"), false).await.unwrap();

    let conn = do_harness_db::connect_and_migrate(dir.path())
        .await
        .unwrap();
    let rows = do_harness_db::list_skill_evals(&conn, "alpha")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pass_rate, Some(1.0));
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_skill_filter_errors() {
    let dir = fixture_root(VALID_SKILL_MD, None);
    let err = eval_run(dir.path(), Some("ghost"), false)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("skill 'ghost' not found under .agents/skills")
    );
}

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use super::*;
use crate::eval_walk::WalkRun;

/// `discover_skills` returns only directories with a `SKILL.md`, sorted.
#[test]
fn discover_skills_filters_and_sorts_by_skill_md() {
    let dir = tempfile::tempdir().unwrap();
    let skills = dir.path().join("skills");
    for name in ["zeta", "alpha"] {
        fs::create_dir_all(skills.join(name)).unwrap();
        fs::write(skills.join(name).join("SKILL.md"), "# skill").unwrap();
    }
    fs::create_dir_all(skills.join("bare")).unwrap();
    fs::write(skills.join("note.txt"), "not a dir").unwrap();

    let found = discover_skills(&skills);
    let names: Vec<String> = found
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["alpha", "zeta"]);
}

/// A missing gate script yields an `Unavailable` verdict, not a skill defect.
#[test]
fn run_structure_gate_is_unavailable_when_script_missing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does_not_exist.py");
    let (verdict, message) = run_structure_gate(dir.path(), &missing);
    assert_eq!(verdict, GateVerdict::Unavailable);
    assert!(message.contains("not found"));
}

/// Documentation assertions (no reserved prefix) are never graded.
#[tokio::test(flavor = "current_thread")]
async fn documentation_only_assertions_are_not_graded() {
    let evals = SkillEvals {
        skill_name: "s".into(),
        evals: vec![EvalCase {
            id: 1,
            prompt: "run it".into(),
            expected_output: "done".into(),
            files: vec![],
            assertions: vec!["this is a human note".to_owned()],
        }],
    };
    let dir = tempfile::tempdir().unwrap();
    let walk = WalkRun {
        present: false,
        success: true,
    };

    let outcome = grade_skill(&evals, dir.path(), &walk).await.unwrap();

    assert_eq!(outcome.graded, 0);
    assert_eq!(outcome.passed, 0);
    assert_eq!(outcome.pass_rate, None);
    assert_eq!(outcome.prompt, None);
}

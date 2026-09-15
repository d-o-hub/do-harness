#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use super::gate::{GateVerdict, run_structure_gate};
use super::grading::{EvalCase, SkillEvals, grade_skill};
use super::orchestrator::discover_skills;
use crate::eval_sandbox::referenced_paths;
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

/// `strip_guidance` must make the payload observably absent, and
/// `guidance_present` must report it back the moment anything restores it.
///
/// The without-skill baseline is only meaningful while the guidance is gone; a
/// walkthrough that re-scaffolds `SKILL.md` (e.g. via `do-harness init`) would
/// otherwise produce a baseline that grades a copy of the guidance and reports
/// a false `+0.00` lift.
#[test]
fn strip_guidance_is_observable_and_detects_regeneration() {
    let real = tempfile::tempdir().unwrap();
    let skill = real.path().join(".agents/skills/demo");
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(skill.join("SKILL.md"), "---\nname: demo\n---\nbody\n").unwrap();
    fs::write(skill.join("references/notes.md"), "notes\n").unwrap();

    let sandbox = crate::eval_sandbox::Sandbox::for_skill(real.path(), &skill, "demo").unwrap();
    assert!(sandbox.guidance_present("demo"));

    sandbox.strip_guidance("demo").unwrap();
    assert!(
        !sandbox.guidance_present("demo"),
        "stripped guidance must read as absent"
    );

    // Simulate an executor regenerating the payload mid-baseline.
    let regenerated = sandbox.root().join(".agents/skills/demo/SKILL.md");
    fs::write(&regenerated, "---\nname: demo\n---\nbody\n").unwrap();
    assert!(
        sandbox.guidance_present("demo"),
        "regenerated guidance must be detected"
    );
}

/// Only concrete repo paths a skill names are mirrored; bare tree prefixes are
/// ignored, because a whole-tree copy is the difference between a sandbox that
/// costs kilobytes and one that costs hundreds of kilobytes per eval case.
/// `integrations/<pkg>` is the one deliberate widening: a package dir is a
/// self-contained tool closure a named repo script reads.
#[test]
fn referenced_paths_selects_concrete_files_only() {
    let dir = tempfile::tempdir().unwrap();
    let skill = dir.path().join("demo");
    fs::create_dir_all(skill.join("evals")).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "Run `bash scripts/publish-npm.sh --dist dist` and read docs/releasing.md.\n\
         The wrapper lives in integrations/npm/platforms/linux-x64/package.json.\n\
         See integrations/ for the layout and scripts/ generally.\n",
    )
    .unwrap();
    let found = referenced_paths(&skill);
    assert!(
        found.contains(&"scripts/publish-npm.sh".to_owned()),
        "{found:?}"
    );
    assert!(found.contains(&"docs/releasing.md".to_owned()), "{found:?}");
    // The package root, not the leaf file: the script reads sibling files.
    assert!(found.contains(&"integrations/npm".to_owned()), "{found:?}");
    assert!(
        !found.contains(&"integrations/npm/platforms/linux-x64/package.json".to_owned()),
        "{found:?}"
    );
    // Bare prefixes must not be mirrored.
    assert!(!found.contains(&"scripts".to_owned()), "{found:?}");
    assert!(!found.contains(&"integrations".to_owned()), "{found:?}");
}

/// A skill that names no repo paths mirrors none, so its sandbox stays free of
/// the non-hidden entries that would flip `init`'s language detection.
#[test]
fn referenced_paths_is_empty_for_a_self_contained_skill() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("evals")).unwrap();
    fs::write(dir.path().join("SKILL.md"), "No repo paths named here.\n").unwrap();
    assert!(referenced_paths(dir.path()).is_empty());
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
            dim: do_harness_types::EvalDim::default(),
            kind: super::fixture::EvalKind::default(),
        }],
    };
    let dir = tempfile::tempdir().unwrap();
    let walk = WalkRun::absent();

    let outcome = grade_skill(&evals, dir.path(), &walk).await.unwrap();

    assert_eq!(outcome.graded, 0);
    assert_eq!(outcome.passed, 0);
    assert_eq!(outcome.pass_rate, None);
    assert_eq!(outcome.prompt, None);
}

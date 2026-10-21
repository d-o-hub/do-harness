//! Existing repositories can refresh only the validator, then evaluate versioned skills.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command;

mod support;

fn harness(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_do-harness"));
    support::isolate_command(&mut command);
    command.arg("--root").arg(root);
    command
}

fn output(command: &mut Command) -> (bool, String, String) {
    let output = command.output().expect("spawn do-harness");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const OLD_VALIDATOR: &str = r#"#!/usr/bin/env python3
import re
import sys
from pathlib import Path
import yaml

content = (Path(sys.argv[1]) / "SKILL.md").read_text()
frontmatter = yaml.safe_load(re.match(r"^---\n(.*?)\n---", content, re.DOTALL).group(1))
allowed = {"name", "description", "license", "allowed-tools", "metadata"}
unexpected = sorted(set(frontmatter) - allowed)
if unexpected:
    print(f"Unexpected key(s) in SKILL.md frontmatter: {', '.join(unexpected)}")
    raise SystemExit(1)
print("Skill is valid!")
"#;

#[tokio::test(flavor = "current_thread")]
async fn refresh_replaces_only_validator_and_unblocks_eval() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    let creator = root.join(".agents/skills/skill-creator");
    let scripts = creator.join("scripts");
    let skill = root.join(".agents/skills/metadata-skill");
    fs::create_dir_all(&scripts).expect("create scripts directory");
    fs::create_dir_all(skill.join("evals")).expect("create skill evals directory");
    let existing_skill_doc = creator.join("SKILL.md");
    let custom_script = scripts.join("custom.py");
    let validator = scripts.join("quick_validate.py");
    fs::write(&existing_skill_doc, "locally customized skill creator\n")
        .expect("write custom skill");
    fs::write(&custom_script, "print('keep')\n").expect("write custom script");
    fs::write(&validator, OLD_VALIDATOR).expect("write old validator");
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: metadata-skill\nversion: 1.1.0\ndescription: Exercise validator refresh for existing repositories.\ncategory: quality\nlicense: MIT\n---\n# Metadata skill\n",
    )
    .expect("write skill");
    fs::write(
        skill.join("evals/evals.json"),
        r#"{
  "skill_name": "metadata-skill",
  "version": "1.1.0",
  "evals": [{
    "id": 1,
    "prompt": "Print the installed harness version.",
    "expected_output": "do-harness version",
    "files": [],
    "assertions": ["cli:version:contains:do-harness"]
  }]
}"#,
    )
    .expect("write versioned eval fixture");

    let mut command = harness(root);
    let (success, stdout, stderr) =
        output(command.args(["eval", "--skill", "metadata-skill", "--no-lift"]));
    assert!(
        !success,
        "old validator unexpectedly accepted skill: {stdout}\n{stderr}"
    );
    assert!(stdout.contains("structure=invalid"), "{stdout}\n{stderr}");
    assert!(stdout.contains("evals=skipped"), "{stdout}\n{stderr}");
    let conn = do_harness_db::connect_and_migrate(root)
        .await
        .expect("connect eval database");
    assert!(
        do_harness_db::list_skill_evals(&conn, "metadata-skill")
            .await
            .expect("read initial score")
            .is_empty(),
        "invalid skill must not persist a score"
    );

    let mut command = harness(root);
    let (success, stdout, stderr) = output(command.args(["skills", "refresh-validator"]));
    assert!(success, "validator refresh failed: {stdout}\n{stderr}");
    assert!(stdout.contains("Refreshed .agents/skills/skill-creator/scripts/quick_validate.py"));
    assert_eq!(
        fs::read_to_string(existing_skill_doc).expect("read custom skill"),
        "locally customized skill creator\n"
    );
    assert_eq!(
        fs::read_to_string(custom_script).expect("read custom script"),
        "print('keep')\n"
    );
    assert!(
        fs::read_to_string(validator)
            .expect("read refreshed validator")
            .contains("category"),
        "refresh must install the bundled validator"
    );

    let mut command = harness(root);
    let (success, stdout, stderr) =
        output(command.args(["eval", "--skill", "metadata-skill", "--no-lift"]));
    assert!(success, "refreshed eval failed: {stdout}\n{stderr}");
    let rows = do_harness_db::list_skill_evals(&conn, "metadata-skill")
        .await
        .expect("read persisted score");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pass_rate, Some(1.0));
}

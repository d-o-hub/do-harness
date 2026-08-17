//! Skill-eval runner for `do-harness eval`.
//!
//! The structure gate is delegated to skill-creator's `quick_validate.py` —
//! the canonical check is never duplicated in Rust. Skills passing the gate
//! have their `evals/evals.json` fixtures parsed; one `skill_evals` row is
//! persisted per skill with `pass_rate` = fraction of structurally valid
//! cases and `token_efficiency` = `None`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Canonical `evals/evals.json` fixture schema.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillEvals {
    /// Fixture skill name; the persisted row uses the directory name.
    #[allow(dead_code)] // schema-required key, not consumed by the runner
    skill_name: String,
    /// Individual evaluation cases.
    evals: Vec<EvalCase>,
}

/// A single evaluation fixture case.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvalCase {
    /// Stable fixture identifier.
    #[allow(dead_code)] // schema-required key, not consumed by the runner
    id: i64,
    /// Prompt handed to the evaluated skill.
    prompt: String,
    /// Expected outcome of the prompt.
    expected_output: String,
    /// Files the case reads or writes.
    #[allow(dead_code)] // schema-required key, not consumed by the runner
    files: Vec<String>,
    /// Assertions the case must satisfy.
    assertions: Vec<String>,
}

/// Result of evaluating a single skill directory.
struct SkillReport {
    /// Whether the skill failed the structure gate.
    gate_failed: bool,
    /// Per-skill summary line printed to stdout.
    line: String,
    /// `pass_rate` to persist; `None` when nothing should be persisted.
    pass_rate: Option<f64>,
}

/// Runs the skill-eval benchmark for every skill under `root/.agents/skills`.
///
/// Each skill directory containing a `SKILL.md` is validated with
/// skill-creator's `quick_validate.py`. When the gate passes and the skill
/// ships `evals/evals.json`, valid fixture cases are counted and a single
/// `skill_evals` row is persisted with `pass_rate` = valid / total. When
/// `skill` is set, only that skill is evaluated; a missing `SKILL.md` is an
/// error.
///
/// # Errors
///
/// Returns an error when a requested skill is not found under
/// `.agents/skills`, when the state database cannot be initialized or
/// written, or when any evaluated skill fails the structure gate (the
/// message lists the failing skill(s)).
pub async fn run_eval(root: &Path, skill: Option<&str>) -> Result<()> {
    let skills_root = root.join(".agents/skills");
    let gate_script = skills_root.join("skill-creator/scripts/quick_validate.py");
    let entries = match skill {
        Some(name) => {
            let dir = skills_root.join(name);
            if !dir.join("SKILL.md").is_file() {
                bail!("skill '{name}' not found under .agents/skills");
            }
            vec![dir]
        }
        None => discover_skills(&skills_root),
    };

    let conn = do_harness_db::connect_and_migrate(root).await?;
    let mut invalid = Vec::new();
    for entry in entries {
        let name = entry
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("invalid skill directory name: {}", entry.display()))?
            .to_owned();
        let report = check_skill(&entry, &name, &gate_script)?;
        println!("{}", report.line);
        if let Some(pass_rate) = report.pass_rate {
            do_harness_db::insert_skill_eval(
                &conn,
                &do_harness_db::NewSkillEval {
                    skill_name: &name,
                    prompt: None,
                    expected_outcome: None,
                    token_efficiency: None,
                    pass_rate: Some(pass_rate),
                },
            )
            .await?;
        }
        if report.gate_failed {
            invalid.push(name);
        }
    }

    if invalid.is_empty() {
        Ok(())
    } else {
        bail!("structure gate failed for skill(s): {}", invalid.join(", "))
    }
}

/// Lists skill directories under `skills_root` that contain a `SKILL.md`,
/// sorted by name.
fn discover_skills(skills_root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let Ok(entries) = fs::read_dir(skills_root) else {
        return dirs;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if dir.is_dir() && dir.join("SKILL.md").is_file() {
            dirs.push(dir);
        }
    }
    dirs.sort();
    dirs
}

/// Structure-gate outcome for a skill directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateVerdict {
    /// `quick_validate.py` passed.
    Pass,
    /// The skill is structurally invalid.
    Fail,
    /// The gate could not run: script or `python3` missing. Not a skill
    /// defect; consumer workspaces without skill-creator hit this.
    Unavailable,
}

/// Runs the canonical structure gate for `dir`.
///
/// Returns the verdict plus a message: the last line of the gate's combined
/// stdout and stderr when it ran, or a diagnostic when it could not.
fn run_structure_gate(dir: &Path, gate_script: &Path) -> (GateVerdict, String) {
    if !gate_script.is_file() {
        return (
            GateVerdict::Unavailable,
            format!("quick_validate.py not found at {}", gate_script.display()),
        );
    }
    let Ok(output) = Command::new("python3").arg(gate_script).arg(dir).output() else {
        return (
            GateVerdict::Unavailable,
            "gate could not be executed (python3 missing)".to_owned(),
        );
    };
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    let message = combined
        .lines()
        .last()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let verdict = if output.status.success() {
        GateVerdict::Pass
    } else {
        GateVerdict::Fail
    };
    (verdict, message)
}

/// Validates the structure gate and, when it does not fail, parses and scores
/// the skill's `evals/evals.json` fixtures.
fn check_skill(dir: &Path, name: &str, gate_script: &Path) -> Result<SkillReport> {
    let (verdict, gate_msg) = run_structure_gate(dir, gate_script);
    if verdict == GateVerdict::Fail {
        return Ok(SkillReport {
            gate_failed: true,
            line: format!("{name}: structure=invalid: {gate_msg} evals=skipped"),
            pass_rate: None,
        });
    }
    let structure = match verdict {
        GateVerdict::Pass => "ok".to_owned(),
        GateVerdict::Unavailable => format!("unknown (gate unavailable: {gate_msg})"),
        GateVerdict::Fail => unreachable!("handled above"),
    };

    let evals_path = dir.join("evals/evals.json");
    let content = match fs::read_to_string(&evals_path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(SkillReport {
                gate_failed: false,
                line: format!("{name}: structure={structure} evals=none"),
                pass_rate: None,
            });
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", evals_path.display()));
        }
    };

    let parsed = match serde_json::from_str::<SkillEvals>(&content) {
        Ok(parsed) => parsed,
        Err(err) => {
            return Ok(SkillReport {
                gate_failed: false,
                line: format!("{name}: structure={structure} evals-invalid: {err}"),
                pass_rate: None,
            });
        }
    };

    let total = u32::try_from(parsed.evals.len()).unwrap_or(u32::MAX);
    let valid = u32::try_from(
        parsed
            .evals
            .iter()
            .filter(|case| is_valid_case(case))
            .count(),
    )
    .unwrap_or(u32::MAX);
    let pass_rate = (total > 0).then(|| f64::from(valid) / f64::from(total));
    let line = match pass_rate {
        Some(rate) => {
            format!("{name}: structure={structure} evals={valid}/{total} pass_rate={rate:.2}")
        }
        None => format!("{name}: structure={structure} evals={valid}/{total}"),
    };
    Ok(SkillReport {
        gate_failed: false,
        line,
        pass_rate,
    })
}

/// A fixture case is valid when its prompt, expected output, and assertions
/// are all non-empty.
fn is_valid_case(case: &EvalCase) -> bool {
    !case.prompt.is_empty() && !case.expected_output.is_empty() && !case.assertions.is_empty()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const VALID_SKILL_MD: &str = "---\nname: test-skill\ndescription: A fixture skill used by the eval-runner tests.\nlicense: MIT\n---\n\n# Test Skill\n";

    const NO_FRONTMATTER_MD: &str = "# No frontmatter here\n";

    const TWO_VALID_ONE_INVALID_EVALS: &str = r#"{
      "skill_name": "test-skill",
      "evals": [
        {"id": 1, "prompt": "prompt one", "expected_output": "out one", "files": [], "assertions": ["a1", "a2"]},
        {"id": 2, "prompt": "prompt two", "expected_output": "out two", "files": ["f.txt"], "assertions": ["b1"]},
        {"id": 3, "prompt": "", "expected_output": "", "files": [], "assertions": []}
      ]
    }"#;

    const ONE_VALID_EVAL: &str = r#"{
      "skill_name": "alpha",
      "evals": [
        {"id": 1, "prompt": "p", "expected_output": "e", "files": [], "assertions": ["a"]}
      ]
    }"#;

    /// Locates the real `quick_validate.py` at the repository root.
    fn gate_script_path() -> PathBuf {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        repo_root.join(".agents/skills/skill-creator/scripts/quick_validate.py")
    }

    /// Builds a tempdir fixture: `.agents/skills/<name>/SKILL.md` plus an
    /// optional `evals/evals.json`, and a copy of the real gate script.
    fn fixture_root(skills: &[(&str, &str, Option<&str>)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let skills_root = dir.path().join(".agents/skills");
        let scripts = skills_root.join("skill-creator/scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::copy(gate_script_path(), scripts.join("quick_validate.py")).unwrap();
        for (name, skill_md, evals) in skills {
            let skill_dir = skills_root.join(name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();
            if let Some(json) = evals {
                fs::create_dir_all(skill_dir.join("evals")).unwrap();
                fs::write(skill_dir.join("evals/evals.json"), json).unwrap();
            }
        }
        dir
    }

    #[tokio::test(flavor = "current_thread")]
    async fn valid_skill_with_partially_invalid_evals_persists_pass_rate() {
        let dir = fixture_root(&[(
            "test-skill",
            VALID_SKILL_MD,
            Some(TWO_VALID_ONE_INVALID_EVALS),
        )]);
        run_eval(dir.path(), None).await.unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let rows = do_harness_db::list_skill_evals(&conn, "test-skill")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].skill_name, "test-skill");
        assert_eq!(rows[0].prompt, None);
        assert_eq!(rows[0].expected_outcome, None);
        assert_eq!(rows[0].token_efficiency, None);
        let pass_rate = rows[0].pass_rate.unwrap();
        assert!((pass_rate - 2.0 / 3.0).abs() < 1e-9);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn skill_without_evals_skips_persistence() {
        let dir = fixture_root(&[("bare-skill", VALID_SKILL_MD, None)]);
        run_eval(dir.path(), None).await.unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        let rows = do_harness_db::list_skill_evals(&conn, "bare-skill")
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn skill_failing_structure_gate_errors() {
        let dir = fixture_root(&[("broken-skill", NO_FRONTMATTER_MD, None)]);
        let err = run_eval(dir.path(), None).await.unwrap_err();
        assert!(err.to_string().contains("broken-skill"));
    }

    /// A consumer workspace without skill-creator has no gate script; the
    /// skills are still evaluated structurally and must not hard-fail.
    #[tokio::test(flavor = "current_thread")]
    async fn missing_gate_script_is_not_a_structure_failure() {
        let dir = tempfile::tempdir().unwrap();
        let skills_root = dir.path().join(".agents/skills");
        let skill_dir = skills_root.join("alpha");
        fs::create_dir_all(skill_dir.join("evals")).unwrap();
        fs::write(skill_dir.join("SKILL.md"), VALID_SKILL_MD).unwrap();
        fs::write(skill_dir.join("evals/evals.json"), ONE_VALID_EVAL).unwrap();

        run_eval(dir.path(), None).await.unwrap();

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
    async fn skill_filter_selects_only_matching_skill() {
        let dir = fixture_root(&[
            ("alpha", VALID_SKILL_MD, Some(ONE_VALID_EVAL)),
            ("beta", VALID_SKILL_MD, None),
        ]);
        run_eval(dir.path(), Some("alpha")).await.unwrap();

        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        assert_eq!(
            do_harness_db::list_skill_evals(&conn, "alpha")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            do_harness_db::list_skill_evals(&conn, "beta")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unknown_skill_filter_errors() {
        let dir = fixture_root(&[("alpha", VALID_SKILL_MD, None)]);
        let err = run_eval(dir.path(), Some("ghost")).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("skill 'ghost' not found under .agents/skills")
        );
    }
}

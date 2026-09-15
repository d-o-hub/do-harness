//! Strike-driven skill scaffolding for `do-harness distill --from-strikes`.
//!
//! AGENTS.md §6 says the steering loop must turn a guide defect into a
//! feedforward guide once a sensor fires more than twice in a sprint, but the
//! recorded strikes were never wired to any action: the counters accumulated in
//! `error_signatures` and nothing read them. This module closes that loop by
//! generating a *starter* skill scaffold, plus a failing fixture, directly from
//! the recorded signature.
//!
//! Two rules keep the starters honest:
//!   * the scaffold is generated from the signature text, never hand-written
//!     prose, so it cannot describe a fix that was never observed;
//!   * the fixture's assertions are *negative* (`absent:` / `not-contains:`),
//!     because an unresolved strike has no passing fix to assert positively.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use do_harness_types::ErrorSignature;

use crate::report::Format;

/// What a strike-driven scaffold run did for one signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaffoldAction {
    /// Files were written.
    Wrote,
    /// Files would be written, but `--dry-run` was set.
    WouldWrite,
    /// Skipped because the skill directory already exists.
    ExistingSkill,
}

/// Outcome of a strike-driven scaffold run.
#[derive(Debug, Clone)]
pub struct ScaffoldOutcome {
    /// Skill name the scaffold was written for.
    pub skill: String,
    /// What happened for this signature.
    pub action: ScaffoldAction,
    /// Number of strikes that triggered the scaffold.
    pub strikes: i64,
    /// Files created or that would be created.
    pub files: Vec<PathBuf>,
}

impl ScaffoldOutcome {
    /// Whether files exist on disk as a result of this run.
    #[must_use]
    pub fn written(&self) -> bool {
        self.action == ScaffoldAction::Wrote
    }
}

/// Returns strikes at or past the steering-loop threshold.
///
/// `threshold` is the AGENTS.md §6 ">2 times in one sprint" rule expressed as
/// `>= 3` recorded strikes; callers pass [`crate::telemetry::FAIL_FAST_STRIKES`]
/// so the fail-fast and distillation thresholds cannot drift apart.
#[must_use]
pub fn striking_signatures(signatures: &[ErrorSignature], threshold: i64) -> Vec<&ErrorSignature> {
    let mut out: Vec<&ErrorSignature> = signatures
        .iter()
        .filter(|signature| signature.attempt_count >= threshold)
        .collect();
    out.sort_by(|a, b| {
        b.attempt_count
            .cmp(&a.attempt_count)
            .then_with(|| a.signature.cmp(&b.signature))
    });
    out
}

/// Derives a skill directory name from a recorded signature.
///
/// `sensor:clippy` becomes `clippy`, and an arbitrary fingerprint becomes a
/// slug. The `sensor:` namespace is stripped because the sensor name is the
/// actionable part; the rest is slugged so the result is a valid skill
/// directory.
#[must_use]
pub fn slug_from_signature(signature: &str) -> String {
    let base = signature.strip_prefix("sensor:").unwrap_or(signature);
    let mut slug = String::with_capacity(base.len());
    let mut last_dash = false;
    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "sensor-strike".to_owned()
    } else {
        slug
    }
}

/// Renders the scaffold `SKILL.md` from the recorded signature.
///
/// Every factual claim in the body comes from the signature row; the guidance
/// is deliberately a stub the author must complete, which is why the fixture
/// asserts absence rather than a fix.
#[must_use]
pub fn render_skill_md(signature: &ErrorSignature, slug: &str) -> String {
    let message = signature
        .message
        .as_deref()
        .unwrap_or("(no diagnostic recorded)");
    format!(
        "---\n\
         name: {slug}\n\
         description: >\n\
         \x20 Recurring sensor failure '{sig}' recorded {count} time(s) without a\n\
         \x20 resolved fix. Use when a run hits this failure signature again, before\n\
         \x20 repeating the same recovery attempt.\n\
         license: MIT\n\
         metadata:\n\
         \x20 short-description: Recovery guide for {slug}\n\
         ---\n\
         \n\
         # {slug}\n\
         \n\
         Generated from a recorded strike, not from a verified fix. Complete it by\n\
         running the failing sensor and writing down what actually made it pass.\n\
         \n\
         ## Recorded failure\n\
         \n\
         - Signature: `{sig}`\n\
         - Consecutive strikes: {count}\n\
         - First recorded: {created}\n\
         - Last diagnostic: {message}\n\
         \n\
         ## What to establish before changing code\n\
         \n\
         1. Reproduce the failure with the exact sensor command and capture its exit code.\n\
         2. Classify it (compile, test, lint, environment, unavailable tool) before editing.\n\
         3. Write the minimal fix for that class only; do not relax a sensor to pass.\n\
         4. Re-run the same sensor and record the command that turned it green.\n\
         \n\
         ## Anti-patterns\n\
         \n\
         - Do not weaken, bless, or re-baseline a fixture to make this signature disappear.\n\
         - Do not retry the identical failed action a third time; change the hypothesis.\n\
         - Do not treat an unavailable tool's SKIP as a pass.\n",
        sig = signature.signature,
        count = signature.attempt_count,
        created = signature.created_at,
    )
}

/// Renders the starter fixture that must FAIL until the skill is completed.
///
/// The assertions are negative on purpose: an unresolved strike has no verified
/// positive outcome, so the honest starter proves only that the recorded wrong
/// actions were avoided. The case is `kind: gotchas`, and a second out-of-scope
/// case is included so the generated fixture satisfies `eval --strict-fixtures`
/// the moment it is written — the author fills in positive knowledge, never the
/// dataset taxonomy.
#[must_use]
pub fn render_evals_json(signature: &ErrorSignature, slug: &str) -> String {
    let log = format!(".agents/skills/{slug}/references/strike-log.md");
    let spec = serde_json::json!({
        "skill_name": slug,
        "evals": [
            {
                "id": 1,
                "prompt": format!(
                    "A sensor keeps failing with the recorded signature '{}'. Work the recovery and record what actually made it pass.",
                    signature.signature
                ),
                "expected_output": format!(
                    "Diagnose the recorded failure '{}' with the exact sensor command, record the recovery in references/strike-log.md, and never relax a sensor or bless a fixture to pass.",
                    signature.signature
                ),
                "files": [],
                "dim": "effectiveness",
                "kind": "gotchas",
                "assertions": [
                    format!("exists:{log}"),
                    format!("not-contains:{log}|relaxed the sensor"),
                    format!("not-contains:{log}|skipped the sensor"),
                    format!("not-contains:{log}|blessed the baseline"),
                ]
            },
            {
                "id": 2,
                "prompt": "Summarize the changelog. There is no sensor failure and no recovery work.",
                "expected_output": "Do not load the recovery guide for unrelated prose.",
                "files": [],
                "dim": "discoverability",
                "kind": "negative",
                "assertions": [
                    format!("absent:.do-harness/strike-{slug}-applied")
                ]
            }
        ]
    });
    let rendered = serde_json::to_string_pretty(&spec).unwrap_or_else(|_| "{}".to_owned());
    format!("{rendered}\n")
}

/// Writes the scaffold for each striking signature.
///
/// Existing skills are never overwritten: a strike whose slug already names a
/// skill is reported and skipped, because clobbering a curated guide would be a
/// worse defect than the strike it came from.
///
/// # Errors
///
/// Returns an error when the database cannot be read or a scaffold file cannot
/// be written.
pub async fn scaffold_from_strikes(
    root: &Path,
    skill_filter: Option<&str>,
    task: Option<i64>,
    threshold: i64,
    dry_run: bool,
    format: Format,
) -> Result<Vec<ScaffoldOutcome>> {
    let conn = do_harness_db::connect_and_migrate(root).await?;
    let signatures = do_harness_db::list_error_signatures(&conn, task).await?;
    let striking = striking_signatures(&signatures, threshold);
    let mut outcomes = Vec::new();
    for signature in striking {
        let slug = slug_from_signature(&signature.signature);
        if let Some(filter) = skill_filter {
            if filter != slug {
                continue;
            }
        }
        let skill_dir = root.join(".agents").join("skills").join(&slug);
        if skill_dir.join("SKILL.md").exists() {
            outcomes.push(ScaffoldOutcome {
                skill: slug.clone(),
                action: ScaffoldAction::ExistingSkill,
                strikes: signature.attempt_count,
                files: Vec::new(),
            });
            continue;
        }
        let files = vec![
            skill_dir.join("SKILL.md"),
            skill_dir.join("evals").join("evals.json"),
        ];
        let action = if dry_run {
            ScaffoldAction::WouldWrite
        } else {
            tokio::fs::create_dir_all(skill_dir.join("evals"))
                .await
                .with_context(|| format!("failed to create {}", skill_dir.display()))?;
            tokio::fs::write(files[0].clone(), render_skill_md(signature, &slug))
                .await
                .with_context(|| format!("failed to write {}", files[0].display()))?;
            tokio::fs::write(files[1].clone(), render_evals_json(signature, &slug))
                .await
                .with_context(|| format!("failed to write {}", files[1].display()))?;
            ScaffoldAction::Wrote
        };
        outcomes.push(ScaffoldOutcome {
            skill: slug.clone(),
            action,
            strikes: signature.attempt_count,
            files,
        });
    }

    if format == Format::Json {
        let json: Vec<serde_json::Value> = outcomes
            .iter()
            .map(|outcome| {
                serde_json::json!({
                    "skill": outcome.skill,
                    "action": match outcome.action {
                        ScaffoldAction::Wrote => "wrote",
                        ScaffoldAction::WouldWrite => "would-write",
                        ScaffoldAction::ExistingSkill => "existing-skill",
                    },
                    "written": outcome.written(),
                    "strikes": outcome.strikes,
                    "files": outcome.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&json)?);
    } else if outcomes.is_empty() {
        println!("No signatures at or past {threshold} strikes; nothing to scaffold");
    } else {
        for outcome in &outcomes {
            let verb = match outcome.action {
                ScaffoldAction::Wrote => "scaffolded",
                ScaffoldAction::WouldWrite => "would scaffold",
                ScaffoldAction::ExistingSkill => "skipped existing skill",
            };
            println!("{verb} {} ({} strike(s))", outcome.skill, outcome.strikes);
        }
    }

    // The event names the tracked guide it created, so the path from the
    // recorded strike to the feedforward guide stays traceable even though the
    // event file itself is local state.
    let created: Vec<&ScaffoldOutcome> = outcomes
        .iter()
        .filter(|outcome| outcome.written())
        .collect();
    if !created.is_empty() {
        let guides: Vec<crate::events::GuideRef> = created
            .iter()
            .map(|outcome| crate::events::GuideRef::skill(&outcome.skill))
            .collect();
        let skills: Vec<&str> = created
            .iter()
            .map(|outcome| outcome.skill.as_str())
            .collect();
        let payload = serde_json::json!({
            "trigger": "steering-loop",
            "threshold": threshold,
            "skills": skills,
        });
        crate::events::write_event(root, "distill-strikes", payload, &guides).await?;
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn signature(key: &str, count: i64) -> ErrorSignature {
        ErrorSignature {
            id: 1,
            signature: key.to_owned(),
            task_id: None,
            attempt_count: count,
            message: Some("boom".to_owned()),
            created_at: 1_789_430_400,
        }
    }

    #[test]
    fn striking_filters_and_sorts_by_strikes() {
        let rows = vec![
            signature("sensor:clippy", 1),
            signature("sensor:test", 3),
            signature("sensor:fmt", 5),
        ];
        let striking = striking_signatures(&rows, 3);
        assert_eq!(striking.len(), 2);
        assert_eq!(striking[0].signature, "sensor:fmt");
        assert_eq!(striking[1].signature, "sensor:test");
    }

    #[test]
    fn slug_strips_sensor_namespace_and_unsafe_chars() {
        assert_eq!(slug_from_signature("sensor:clippy"), "clippy");
        assert_eq!(
            slug_from_signature("sensor:eval_walk::tests::x"),
            "eval-walk-tests-x"
        );
        assert!(!slug_from_signature("!!!").is_empty());
        assert_eq!(slug_from_signature("sensor:!!!"), "sensor-strike");
    }

    #[test]
    fn rendered_skill_md_carries_the_recorded_evidence() {
        let sig = signature("sensor:test", 4);
        let md = render_skill_md(&sig, "test");
        assert!(md.contains("name: test"));
        assert!(md.contains("`sensor:test`"));
        assert!(md.contains("Consecutive strikes: 4"));
        assert!(md.contains("boom"));
    }

    #[test]
    fn rendered_fixture_is_gate_clean_and_demands_the_unresolved_log() {
        let sig = signature("sensor:test", 4);
        let fixture = render_evals_json(&sig, "test");
        let parsed: serde_json::Value = serde_json::from_str(&fixture).unwrap();
        let cases = parsed["evals"].as_array().unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0]["kind"], "gotchas");
        assert_eq!(cases[1]["kind"], "negative");

        let gotchas: Vec<&str> = cases[0]["assertions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        // The starter fails because the recovery log does not exist yet; that
        // `exists:` is what makes the scaffold an honest red fixture.
        assert!(gotchas.iter().any(|a| a.starts_with("exists:")));
        assert!(gotchas.iter().any(|a| a.starts_with("not-contains:")));

        // Taxonomy check: every case must be graded, and the suite must carry
        // the out-of-scope case --strict-fixtures requires. Both come from
        // crate::eval::fixture::fixture_diagnostics, so this asserts with the
        // real diagnostic instead of a hand-copied rule.
        let all_graded = cases.iter().all(|case| {
            case["assertions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| crate::eval_assert::is_graded(a.as_str().unwrap()))
        });
        assert!(all_graded);
        assert!(cases.iter().any(|case| case["kind"] == "negative"));
        assert!(!fixture.contains("PLACEHOLDER"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn scaffold_skips_an_existing_skill_and_dry_run_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let conn = do_harness_db::connect_and_migrate(dir.path())
            .await
            .unwrap();
        for _ in 0..3 {
            do_harness_db::bump_error_signature(&conn, "sensor:clippy", None, Some("boom"))
                .await
                .unwrap();
        }
        // A curated skill with that slug must not be overwritten.
        let existing = dir.path().join(".agents/skills/clippy");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("SKILL.md"), "---\nname: clippy\n---\n").unwrap();

        let outcomes = scaffold_from_strikes(
            dir.path(),
            None,
            None,
            3,
            false,
            crate::report::Format::Json,
        )
        .await
        .unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].action, ScaffoldAction::ExistingSkill);
        assert_eq!(
            std::fs::read_to_string(existing.join("SKILL.md")).unwrap(),
            "---\nname: clippy\n---\n"
        );

        // A fresh signature scaffolds, and dry-run leaves no files.
        do_harness_db::clear_error_signatures(&conn, None, None)
            .await
            .unwrap();
        for _ in 0..3 {
            do_harness_db::bump_error_signature(&conn, "sensor:shell", None, Some("boom"))
                .await
                .unwrap();
        }
        let dry =
            scaffold_from_strikes(dir.path(), None, None, 3, true, crate::report::Format::Json)
                .await
                .unwrap();
        assert_eq!(dry.len(), 1);
        assert_eq!(dry[0].action, ScaffoldAction::WouldWrite);
        assert!(!dir.path().join(".agents/skills/shell/SKILL.md").exists());

        let real = scaffold_from_strikes(
            dir.path(),
            None,
            None,
            3,
            false,
            crate::report::Format::Json,
        )
        .await
        .unwrap();
        assert_eq!(real[0].action, ScaffoldAction::Wrote);
        assert!(dir.path().join(".agents/skills/shell/SKILL.md").exists());
        assert!(
            dir.path()
                .join(".agents/skills/shell/evals/evals.json")
                .exists()
        );
    }
}

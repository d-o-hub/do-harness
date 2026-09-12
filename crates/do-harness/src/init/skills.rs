//! Portable skill scaffolding for `do-harness init`.

use std::path::Path;

use anyhow::Result;

use super::{InitOpts, InitReport, write_if_absent};

/// Portable skill templates written into `.agents/skills/<name>/SKILL.md`.
///
/// Only skills with operational value to an adopting project are scaffolded.
/// The do-harness development methodology skills (htn-planner, spike-runner,
/// event-modeler, skill-distiller) remain in this repository's own
/// `.agents/skills/` for developing do-harness, but are deliberately not
/// forced onto consumers.
pub(super) struct SkillSpec {
    pub(super) name: &'static str,
    pub(super) skill_md: &'static str,
    pub(super) evals: &'static str,
    pub(super) walkthrough: Option<&'static str>,
    pub(super) references: &'static [(&'static str, &'static str)],
}

pub(super) const SKILLS: &[SkillSpec] = &[SkillSpec {
    name: "harness",
    skill_md: include_str!("../../templates/skills/harness/SKILL.md"),
    evals: include_str!("../../templates/skills/harness/evals/evals.json"),
    walkthrough: Some(include_str!(
        "../../templates/skills/harness/evals/walkthrough.sh"
    )),
    references: &[(
        "references/heuristics.md",
        include_str!("../../templates/skills/harness/references/heuristics.md"),
    )],
}];

/// skill-creator ships its scaffolding script and the structure gate so that a
/// consumer's `do-harness eval` can run the real `quick_validate.py`.
const SKILL_CREATOR_MD: &str = include_str!("../../templates/skills/skill-creator/SKILL.md");
const SKILL_CREATOR_INIT: &str =
    include_str!("../../templates/skills/skill-creator/scripts/init_skill.py");
const SKILL_CREATOR_QUICK_VALIDATE: &str =
    include_str!("../../templates/skills/skill-creator/scripts/quick_validate.py");
const SKILL_CREATOR_OPENAI_YAML: &str =
    include_str!("../../templates/skills/skill-creator/references/openai_yaml.md");
const SKILL_CREATOR_GENERATE_YAML: &str =
    include_str!("../../templates/skills/skill-creator/scripts/generate_openai_yaml.py");

/// Writes the portable skill subset (harness + skill-creator).
pub(super) fn scaffold_skills(root: &Path, opts: &InitOpts, report: &mut InitReport) -> Result<()> {
    for spec in SKILLS {
        let skill_dir = format!(".agents/skills/{}", spec.name);
        let skill_md = format!("{skill_dir}/SKILL.md");
        // Count a skill as scaffolded only when its SKILL.md is actually
        // written (fresh, or overwritten via --force) — not when skipped.
        let skill_md_written = opts.force || !root.join(&skill_md).exists();
        write_if_absent(root, &skill_md, spec.skill_md, opts.force, report)?;
        if skill_md_written {
            report.skills += 1;
        }
        write_if_absent(
            root,
            &format!("{skill_dir}/evals/evals.json"),
            spec.evals,
            opts.force,
            report,
        )?;
        if let Some(walkthrough) = spec.walkthrough {
            write_if_absent(
                root,
                &format!("{skill_dir}/evals/walkthrough.sh"),
                walkthrough,
                opts.force,
                report,
            )?;
            crate::fs_perm::set_owner_exec(
                &root.join(format!("{skill_dir}/evals/walkthrough.sh")),
            )?;
        }
        for (rel, contents) in spec.references {
            write_if_absent(
                root,
                &format!("{skill_dir}/{rel}"),
                contents,
                opts.force,
                report,
            )?;
        }
    }

    write_if_absent(
        root,
        ".agents/skills/skill-creator/SKILL.md",
        SKILL_CREATOR_MD,
        opts.force,
        report,
    )?;
    write_if_absent(
        root,
        ".agents/skills/skill-creator/scripts/init_skill.py",
        SKILL_CREATOR_INIT,
        opts.force,
        report,
    )?;
    write_if_absent(
        root,
        ".agents/skills/skill-creator/scripts/quick_validate.py",
        SKILL_CREATOR_QUICK_VALIDATE,
        opts.force,
        report,
    )?;
    write_if_absent(
        root,
        ".agents/skills/skill-creator/references/openai_yaml.md",
        SKILL_CREATOR_OPENAI_YAML,
        opts.force,
        report,
    )?;
    write_if_absent(
        root,
        ".agents/skills/skill-creator/scripts/generate_openai_yaml.py",
        SKILL_CREATOR_GENERATE_YAML,
        opts.force,
        report,
    )?;
    crate::fs_perm::set_owner_exec(
        &root.join(".agents/skills/skill-creator/scripts/quick_validate.py"),
    )?;
    Ok(())
}

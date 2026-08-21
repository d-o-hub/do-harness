//! `do-harness init`: scaffold a harness workspace in a consumer repository.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Executable bits (owner read/write/execute) applied to installed scripts.
const OWNER_EXEC_MASK: u32 = 0o111;

/// `.gitignore` entries the harness needs; appended, never clobbered.
const GITIGNORE_ENTRIES: &str = ".do-harness/\n.agents/events/\n";

/// Target language for the scaffolded sensor pack.
#[derive(Debug, Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum Language {
    /// Rust sensor pack (fmt/check/clippy/test/loc) plus a check-loc script.
    Rust,
    /// No built-in sensors; commented sensor stubs to fill in.
    Generic,
}

/// Options for [`init_workspace`].
#[derive(Debug, Clone)]
pub struct InitOpts {
    /// Which language pack to scaffold.
    pub language: Language,
    /// Overwrite existing files.
    pub force: bool,
}

/// Files written or skipped during an init run.
#[derive(Debug, Default)]
pub struct InitReport {
    /// Relative paths written.
    pub written: Vec<String>,
    /// Relative paths that already existed and were left untouched.
    pub skipped: Vec<String>,
    /// Invariants upserted into the state database.
    pub seeded: usize,
    /// Number of skills scaffolded (SKILL.md written).
    pub skills: usize,
}

const AGENTS_TEMPLATE: &str = include_str!("../templates/AGENTS.md");
const CONFIG_RUST: &str = include_str!("../templates/do-harness.toml.rust");
const CONFIG_GENERIC: &str = include_str!("../templates/do-harness.toml.generic");
const INVARIANTS_RUST: &str = include_str!("../templates/plans/invariants.json.rust");
const INVARIANTS_GENERIC: &str = include_str!("../templates/plans/invariants.json.generic");
const CHECK_LOC: &str = include_str!("../templates/scripts/check-loc.sh");
const CHECK_COMMITLINT: &str = include_str!("../templates/scripts/check-commitlint.sh");
const CRATE_MANIFEST: &str = include_str!("../templates/crate/Cargo.toml");
const CRATE_LIB: &str = include_str!("../templates/crate/src/lib.rs");

/// Portable skill templates written into `.agents/skills/<name>/SKILL.md`.
struct SkillSpec {
    name: &'static str,
    skill_md: &'static str,
    evals: &'static str,
    walkthrough: Option<&'static str>,
}

const SKILLS: &[SkillSpec] = &[
    SkillSpec {
        name: "harness",
        skill_md: include_str!("../templates/skills/harness/SKILL.md"),
        evals: include_str!("../templates/skills/harness/evals/evals.json"),
        walkthrough: Some(include_str!(
            "../templates/skills/harness/evals/walkthrough.sh"
        )),
    },
    SkillSpec {
        name: "htn-planner",
        skill_md: include_str!("../templates/skills/htn-planner/SKILL.md"),
        evals: include_str!("../templates/skills/htn-planner/evals/evals.json"),
        walkthrough: Some(include_str!(
            "../templates/skills/htn-planner/evals/walkthrough.sh"
        )),
    },
    SkillSpec {
        name: "spike-runner",
        skill_md: include_str!("../templates/skills/spike-runner/SKILL.md"),
        evals: include_str!("../templates/skills/spike-runner/evals/evals.json"),
        walkthrough: Some(include_str!(
            "../templates/skills/spike-runner/evals/walkthrough.sh"
        )),
    },
    SkillSpec {
        name: "skill-distiller",
        skill_md: include_str!("../templates/skills/skill-distiller/SKILL.md"),
        evals: include_str!("../templates/skills/skill-distiller/evals/evals.json"),
        walkthrough: Some(include_str!(
            "../templates/skills/skill-distiller/evals/walkthrough.sh"
        )),
    },
    SkillSpec {
        name: "event-modeler",
        skill_md: include_str!("../templates/skills/event-modeler/SKILL.md"),
        evals: include_str!("../templates/skills/event-modeler/evals/evals.json"),
        walkthrough: Some(include_str!(
            "../templates/skills/event-modeler/evals/walkthrough.sh"
        )),
    },
];

/// skill-creator ships its scaffolding script and the structure gate so that a
/// consumer's `do-harness eval` can run the real `quick_validate.py`.
const SKILL_CREATOR_MD: &str = include_str!("../templates/skills/skill-creator/SKILL.md");
const SKILL_CREATOR_INIT: &str =
    include_str!("../templates/skills/skill-creator/scripts/init_skill.py");
const SKILL_CREATOR_QUICK_VALIDATE: &str =
    include_str!("../templates/skills/skill-creator/scripts/quick_validate.py");

/// Scaffolds a harness workspace in `root`, then initializes the state
/// database and seeds the invariants.
///
/// Existing files are left untouched unless `opts.force` is set; `.gitignore`
/// is appended to rather than overwritten.
///
/// # Errors
///
/// Returns an error when a file cannot be written, the database cannot be
/// initialized, or `plans/invariants.json` does not match the decision-header
/// schema.
pub async fn init_workspace(root: &Path, opts: &InitOpts) -> Result<InitReport> {
    let mut report = InitReport::default();

    validate_existing_invariants(root, opts)?;

    write_if_absent(root, "AGENTS.md", AGENTS_TEMPLATE, opts.force, &mut report)?;
    let config = match opts.language {
        Language::Rust => CONFIG_RUST,
        Language::Generic => CONFIG_GENERIC,
    };
    write_if_absent(root, "do-harness.toml", config, opts.force, &mut report)?;
    let invariants = match opts.language {
        Language::Rust => INVARIANTS_RUST,
        Language::Generic => INVARIANTS_GENERIC,
    };
    write_if_absent(
        root,
        "plans/invariants.json",
        invariants,
        opts.force,
        &mut report,
    )?;
    if opts.language == Language::Rust {
        write_if_absent(
            root,
            "scripts/check-loc.sh",
            CHECK_LOC,
            opts.force,
            &mut report,
        )?;
        make_executable(&root.join("scripts/check-loc.sh"))?;
        write_if_absent(
            root,
            "scripts/check-commitlint.sh",
            CHECK_COMMITLINT,
            opts.force,
            &mut report,
        )?;
        make_executable(&root.join("scripts/check-commitlint.sh"))?;
        scaffold_crate(root, &mut report)?;
    }
    for spec in SKILLS {
        let skill_dir = format!(".agents/skills/{}", spec.name);
        let skill_md = format!("{skill_dir}/SKILL.md");
        // Count a skill as scaffolded only when its SKILL.md is actually
        // written (fresh, or overwritten via --force) — not when skipped.
        let skill_md_written = opts.force || !root.join(&skill_md).exists();
        write_if_absent(root, &skill_md, spec.skill_md, opts.force, &mut report)?;
        if skill_md_written {
            report.skills += 1;
        }
        write_if_absent(
            root,
            &format!("{skill_dir}/evals/evals.json"),
            spec.evals,
            opts.force,
            &mut report,
        )?;
        if let Some(walkthrough) = spec.walkthrough {
            write_if_absent(
                root,
                &format!("{skill_dir}/evals/walkthrough.sh"),
                walkthrough,
                opts.force,
                &mut report,
            )?;
            make_executable(&root.join(format!("{skill_dir}/evals/walkthrough.sh")))?;
        }
    }

    write_if_absent(
        root,
        ".agents/skills/skill-creator/SKILL.md",
        SKILL_CREATOR_MD,
        opts.force,
        &mut report,
    )?;
    write_if_absent(
        root,
        ".agents/skills/skill-creator/scripts/init_skill.py",
        SKILL_CREATOR_INIT,
        opts.force,
        &mut report,
    )?;
    write_if_absent(
        root,
        ".agents/skills/skill-creator/scripts/quick_validate.py",
        SKILL_CREATOR_QUICK_VALIDATE,
        opts.force,
        &mut report,
    )?;
    make_executable(&root.join(".agents/skills/skill-creator/scripts/quick_validate.py"))?;
    append_gitignore(root, &mut report)?;

    report.seeded = seed_invariants(root).await?;
    Ok(report)
}

/// Validates a pre-existing `plans/invariants.json` BEFORE touching the
/// tree: a stale file would otherwise fail the seed step after ~20 files
/// have already been scaffolded.
fn validate_existing_invariants(root: &Path, opts: &InitOpts) -> Result<()> {
    if opts.force {
        return Ok(());
    }
    let path = root.join("plans/invariants.json");
    let Ok(existing) = fs::read_to_string(&path) else {
        return Ok(());
    };
    serde_json::from_str::<Vec<do_harness_types::DecisionHeader>>(&existing).context(format!(
        "pre-existing {} does not match the DecisionHeader schema; fix or remove it before \
         running init",
        path.display()
    ))?;
    Ok(())
}

/// Upserts `plans/invariants.json` into the state database.
async fn seed_invariants(root: &Path) -> Result<usize> {
    let json_path = root.join("plans/invariants.json");
    let json = fs::read_to_string(&json_path)
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let headers: Vec<do_harness_types::DecisionHeader> = serde_json::from_str(&json)
        .context("invalid plans/invariants.json: does not match DecisionHeader schema")?;
    let conn = do_harness_db::connect_and_migrate(root).await?;
    Ok(do_harness_db::seed_invariants(&conn, &headers).await?)
}

/// Writes `body` to `root/relative`, skipping existing files unless `force`.
fn write_if_absent(
    root: &Path,
    relative: &str,
    body: &str,
    force: bool,
    report: &mut InitReport,
) -> Result<()> {
    let path = root.join(relative);
    if path.exists() && !force {
        report.skipped.push(relative.to_owned());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))?;
    report.written.push(relative.to_owned());
    Ok(())
}

/// Writes a minimal cargo crate when no `Cargo.toml` exists so a greenfield
/// workspace passes the rust sensor pack immediately (`init && verify` is
/// green on an empty tree).
///
/// Existing crates are never touched — not even with `--force` — because
/// overwriting a real manifest would be destructive; the crate files are
/// greenfield-only scaffolding.
fn scaffold_crate(root: &Path, report: &mut InitReport) -> Result<()> {
    if root.join("Cargo.toml").exists() {
        report.skipped.push("Cargo.toml".to_owned());
        return Ok(());
    }
    write_if_absent(root, "Cargo.toml", CRATE_MANIFEST, false, report)?;
    write_if_absent(root, "src/lib.rs", CRATE_LIB, false, report)?;
    Ok(())
}

/// Appends the harness `.gitignore` entries when missing; creates the file
/// when it does not exist yet.
fn append_gitignore(root: &Path, report: &mut InitReport) -> Result<()> {
    let path = root.join(".gitignore");
    let Ok(existing) = fs::read_to_string(&path) else {
        fs::write(&path, GITIGNORE_ENTRIES)
            .with_context(|| format!("failed to write {}", path.display()))?;
        report.written.push(".gitignore".to_owned());
        return Ok(());
    };
    let existing_lines: std::collections::HashSet<&str> = existing.lines().collect();
    // Exact-line matching: a substring hit (e.g. `.do-harness-old/` or a
    // comment mentioning the dir) must not suppress the real entry.
    let missing: Vec<&str> = GITIGNORE_ENTRIES
        .lines()
        .filter(|line| !line.is_empty() && !existing_lines.contains(line))
        .collect();
    if missing.is_empty() {
        report.skipped.push(".gitignore".to_owned());
        return Ok(());
    }
    let mut updated = existing;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    for entry in &missing {
        updated.push_str(entry);
        updated.push('\n');
    }
    fs::write(&path, updated).with_context(|| format!("failed to write {}", path.display()))?;
    report.written.push(".gitignore".to_owned());
    Ok(())
}

/// Adds owner execute permission to `path` (unix only).
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    let mode = permissions.mode() | OWNER_EXEC_MASK;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to chmod {}", path.display()))
}

/// No-op on non-unix platforms.
#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests;

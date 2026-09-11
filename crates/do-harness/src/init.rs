//! `do-harness init`: scaffold a harness workspace in a consumer repository.
//!
//! Init is evidence-driven: it detects what the repository is, probes the
//! candidate sensors' tooling, generates a contract containing only proven
//! signals, then executes that contract once. A red baseline is surfaced
//! (and fails the command) instead of presenting the repository as ready.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub mod baseline;
pub mod detect;
pub mod report;

pub use baseline::{Baseline, BaselineState};
pub use detect::{Candidate, CandidateStatus};
pub use report::print_report;

/// `.gitignore` entries the harness needs; appended, never clobbered.
const GITIGNORE_ENTRIES: &str = ".do-harness/\n.agents/events/\n";

/// Target language for the scaffolded sensor pack.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, serde::Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Rust sensor pack (fmt/check/clippy/test/loc) plus check scripts.
    #[default]
    Rust,
    /// No built-in sensors; commented sensor stubs to fill in.
    Generic,
}

/// Options for [`init_workspace`].
///
/// Each boolean maps 1:1 to an independent CLI flag (`--force`, `--no-seed`,
/// `--minimal`, `--no-gitignore`); they are not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct InitOpts {
    /// Explicit language pack; `None` detects it from the repository.
    pub language: Option<Language>,
    /// Overwrite existing files.
    pub force: bool,
    /// Skip seeding `plans/invariants.json` into the state database.
    pub no_seed: bool,
    /// Skip skill scaffolding.
    pub minimal: bool,
    /// Skip creating/modifying `.gitignore`.
    pub no_gitignore: bool,
}

/// Files written or skipped during an init run.
#[derive(Debug, Default, serde::Serialize)]
pub struct InitReport {
    /// Relative paths written.
    pub written: Vec<String>,
    /// Relative paths that already existed and were left untouched.
    pub skipped: Vec<String>,
    /// Invariants upserted into the state database.
    pub seeded: usize,
    /// Number of skills scaffolded (SKILL.md written).
    pub skills: usize,
    /// Resolved language pack.
    pub language: Language,
    /// Human-readable repository facts.
    pub detected: Vec<String>,
    /// Probed candidate signals for the resolved pack.
    pub candidates: Vec<Candidate>,
    /// Outcome of executing the generated contract once; filled by
    /// [`run_baseline`] (the CLI always runs it).
    pub baseline: Option<Baseline>,
}

const AGENTS_TEMPLATE: &str = include_str!("../templates/AGENTS.md");
const CONFIG_GENERIC: &str = include_str!("../templates/do-harness.toml.generic");
const INVARIANTS_RUST: &str = include_str!("../templates/plans/invariants.json.rust");
const INVARIANTS_GENERIC: &str = include_str!("../templates/plans/invariants.json.generic");
const CHECK_LOC: &str = include_str!("../templates/scripts/check-loc.sh");
const CHECK_COMMITLINT: &str = include_str!("../templates/scripts/check-commitlint.sh");
const CHECK_DEPS: &str = include_str!("../templates/scripts/check-deps.sh");
const CHECK_AUDIT: &str = include_str!("../templates/scripts/check-audit.sh");
const CRATE_MANIFEST: &str = include_str!("../templates/crate/Cargo.toml");
const CRATE_LIB: &str = include_str!("../templates/crate/src/lib.rs");

/// Portable skill templates written into `.agents/skills/<name>/SKILL.md`.
///
/// Only skills with operational value to an adopting project are scaffolded.
/// The do-harness development methodology skills (htn-planner, spike-runner,
/// event-modeler, skill-distiller) remain in this repository's own
/// `.agents/skills/` for developing do-harness, but are deliberately not
/// forced onto consumers.
struct SkillSpec {
    name: &'static str,
    skill_md: &'static str,
    evals: &'static str,
    walkthrough: Option<&'static str>,
}

const SKILLS: &[SkillSpec] = &[SkillSpec {
    name: "harness",
    skill_md: include_str!("../templates/skills/harness/SKILL.md"),
    evals: include_str!("../templates/skills/harness/evals/evals.json"),
    walkthrough: Some(include_str!(
        "../templates/skills/harness/evals/walkthrough.sh"
    )),
}];

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
/// The language pack is detected from repository reality unless
/// `opts.language` requests one explicitly. For Rust, the generated config
/// includes only the probes that passed; missing required tooling is
/// surfaced and omitted. Existing files are left untouched unless
/// `opts.force` is set; `.gitignore` is appended to rather than overwritten.
///
/// The baseline execution is a separate step ([`run_baseline`]) so callers
/// can scaffold without running the suite.
///
/// # Errors
///
/// Returns an error when a file cannot be written, the database cannot be
/// initialized, or `plans/invariants.json` does not match the decision-header
/// schema.
pub async fn init_workspace(root: &Path, opts: &InitOpts) -> Result<InitReport> {
    let existing_language = if root.join("do-harness.toml").exists() {
        crate::config::load(root, None)
            .await
            .ok()
            .and_then(|cfg| cfg.language)
    } else {
        None
    };
    let detection = detect::inspect(root, opts.language, existing_language.as_deref());
    let mut report = InitReport {
        language: detection.language,
        detected: detection.findings,
        candidates: detection.candidates,
        ..Default::default()
    };

    validate_existing_invariants(root, opts)?;

    write_if_absent(root, "AGENTS.md", AGENTS_TEMPLATE, opts.force, &mut report)?;
    let config = match report.language {
        Language::Rust => {
            let sensors = detect::included_specs(report.language, &report.candidates);
            generate_rust_config(&sensors)?
        }
        Language::Generic => CONFIG_GENERIC.to_owned(),
    };
    write_if_absent(root, "do-harness.toml", &config, opts.force, &mut report)?;
    let invariants = match report.language {
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
    if report.language == Language::Rust {
        scaffold_scripts(root, opts, &mut report)?;
        scaffold_crate(root, &mut report)?;
    }
    if !opts.minimal {
        scaffold_skills(root, opts, &mut report)?;
    }
    if !opts.no_gitignore {
        append_gitignore(root, &mut report)?;
    }

    if !opts.no_seed {
        report.seeded = seed_invariants(root, false).await?;
    }
    Ok(report)
}

/// Runs the effective contract once and stores the outcome in `report`.
///
/// # Errors
///
/// Returns an error when the config cannot be loaded or executed.
pub async fn run_baseline(root: &Path, report: &mut InitReport) -> Result<()> {
    let (cfg, _) = crate::config::load_raw(root, None).await?;
    report.baseline = Some(baseline::run(&cfg, root)?);
    Ok(())
}

/// Renders the Rust `do-harness.toml` for the included sensors.
///
/// Signal sets are derived from the pack: feedback is the fast subset,
/// verification/release cover every included sensor. Hooks are filtered to
/// included sensors too, so a missing tool cannot make a hook fail.
fn generate_rust_config(sensors: &[crate::config::SensorSpec]) -> Result<String> {
    let names: Vec<&str> = sensors.iter().map(|spec| spec.name.as_str()).collect();
    let mut signal_sets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let feedback = ["fmt", "check", "clippy"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();
    let full = names.iter().map(ToString::to_string).collect::<Vec<_>>();
    signal_sets.insert("feedback".to_owned(), feedback);
    signal_sets.insert("verification".to_owned(), full.clone());
    signal_sets.insert("release".to_owned(), full);
    let pre_commit = ["fmt", "loc"]
        .iter()
        .filter(|name| names.contains(name))
        .map(ToString::to_string)
        .collect();
    let cfg = crate::config::Config {
        language: Some("rust".to_owned()),
        hooks: crate::config::HooksConfig {
            pre_commit,
            pre_push: Vec::new(),
        },
        signal_sets,
        sensors: sensors.to_vec(),
    };
    let body = toml::to_string(&cfg).context("failed to render generated config")?;
    Ok(format!(
        "# do-harness.toml — generated by `do-harness init` from detected tooling.\n\
         # Edit freely; `do-harness explain --set verification --changed` shows what applies.\n{body}"
    ))
}

/// Writes the portable skill subset (harness + skill-creator).
fn scaffold_skills(root: &Path, opts: &InitOpts, report: &mut InitReport) -> Result<()> {
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
    crate::fs_perm::set_owner_exec(
        &root.join(".agents/skills/skill-creator/scripts/quick_validate.py"),
    )?;
    Ok(())
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
pub(crate) async fn seed_invariants(root: &Path, prune: bool) -> Result<usize> {
    let json_path = root.join("plans/invariants.json");
    let json = tokio::fs::read_to_string(&json_path)
        .await
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let headers: Vec<do_harness_types::DecisionHeader> = serde_json::from_str(&json)
        .context("invalid plans/invariants.json: does not match DecisionHeader schema")?;
    let conn = do_harness_db::connect_and_migrate(root).await?;
    Ok(do_harness_db::seed_invariants(&conn, &headers, prune).await?)
}

/// Writes the Rust-pack helper scripts and marks them executable.
fn scaffold_scripts(root: &Path, opts: &InitOpts, report: &mut InitReport) -> Result<()> {
    for (relative, body) in [
        ("scripts/check-loc.sh", CHECK_LOC),
        ("scripts/check-commitlint.sh", CHECK_COMMITLINT),
        ("scripts/check-deps.sh", CHECK_DEPS),
        ("scripts/check-audit.sh", CHECK_AUDIT),
    ] {
        write_if_absent(root, relative, body, opts.force, report)?;
        crate::fs_perm::set_owner_exec(&root.join(relative))?;
    }
    Ok(())
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

#[cfg(test)]
mod tests;

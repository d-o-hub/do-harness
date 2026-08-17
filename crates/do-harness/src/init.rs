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
}

const AGENTS_TEMPLATE: &str = include_str!("../templates/AGENTS.md");
const CONFIG_RUST: &str = include_str!("../templates/do-harness.toml.rust");
const CONFIG_GENERIC: &str = include_str!("../templates/do-harness.toml.generic");
const INVARIANTS_RUST: &str = include_str!("../templates/plans/invariants.json.rust");
const INVARIANTS_GENERIC: &str = include_str!("../templates/plans/invariants.json.generic");
const CHECK_LOC: &str = include_str!("../templates/scripts/check-loc.sh");

/// Portable skill templates written into `.agents/skills/<name>/SKILL.md`.
const SKILLS: &[(&str, &str)] = &[
    (
        "harness",
        include_str!("../templates/skills/harness/SKILL.md"),
    ),
    (
        "htn-planner",
        include_str!("../templates/skills/htn-planner/SKILL.md"),
    ),
    (
        "spike-runner",
        include_str!("../templates/skills/spike-runner/SKILL.md"),
    ),
    (
        "skill-distiller",
        include_str!("../templates/skills/skill-distiller/SKILL.md"),
    ),
    (
        "event-modeler",
        include_str!("../templates/skills/event-modeler/SKILL.md"),
    ),
];

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
    }
    for (name, body) in SKILLS {
        let relative = format!(".agents/skills/{name}/SKILL.md");
        write_if_absent(root, &relative, body, opts.force, &mut report)?;
    }
    append_gitignore(root, &mut report)?;

    report.seeded = seed_invariants(root).await?;
    Ok(report)
}

/// Upserts `plans/invariants.json` into the state database.
async fn seed_invariants(root: &Path) -> Result<usize> {
    let json_path = root.join("plans/invariants.json");
    let json = fs::read_to_string(&json_path)
        .with_context(|| format!("failed to read {}", json_path.display()))?;
    let headers: Vec<do_harness_types::DecisionHeader> = serde_json::from_str(&json)
        .context("invalid plans/invariants.json: does not match DecisionHeader schema")?;
    let conn = do_harness_db::connect_and_migrate(root).await?;
    do_harness_db::seed_invariants(&conn, &headers).await
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
    let missing: Vec<&str> = GITIGNORE_ENTRIES
        .lines()
        .filter(|line| !line.is_empty() && !existing.contains(line))
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
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn init_rust_scaffolds_full_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let opts = InitOpts {
            language: Language::Rust,
            force: false,
        };

        let report = init_workspace(dir.path(), &opts).await.unwrap();

        assert!(report.written.contains(&"AGENTS.md".to_owned()));
        assert!(report.written.contains(&"do-harness.toml".to_owned()));
        assert!(report.written.contains(&"plans/invariants.json".to_owned()));
        assert!(report.written.contains(&"scripts/check-loc.sh".to_owned()));
        assert!(
            report
                .written
                .contains(&".agents/skills/harness/SKILL.md".to_owned())
        );
        assert_eq!(report.seeded, 3);
        let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
        assert!(config.contains("language = \"rust\""));
        assert!(dir.path().join(".do-harness/agent_state.db").exists());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn init_generic_has_no_loc_script() {
        let dir = tempfile::tempdir().unwrap();
        let opts = InitOpts {
            language: Language::Generic,
            force: false,
        };

        let report = init_workspace(dir.path(), &opts).await.unwrap();

        assert!(!report.written.iter().any(|p| p == "scripts/check-loc.sh"));
        let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
        assert!(config.contains("language = \"generic\""));
        assert_eq!(report.seeded, 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn init_is_idempotent_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let opts = InitOpts {
            language: Language::Rust,
            force: false,
        };
        init_workspace(dir.path(), &opts).await.unwrap();

        let report = init_workspace(dir.path(), &opts).await.unwrap();

        assert!(report.written.is_empty());
        assert!(report.skipped.contains(&"do-harness.toml".to_owned()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn init_appends_gitignore_entries() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "# existing\n").unwrap();
        let opts = InitOpts {
            language: Language::Generic,
            force: false,
        };

        init_workspace(dir.path(), &opts).await.unwrap();

        let text = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(text.contains(".do-harness/"));
        assert!(text.contains(".agents/events/"));
        assert!(text.contains("# existing"));
    }
}

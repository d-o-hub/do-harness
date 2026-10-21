//! Unit tests for `do-harness init` (`init.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::skills::SKILLS;
use super::*;

mod node;
mod release;
mod release_package_json;
mod release_published;
mod skills;
mod web;

/// Default init options with an explicit language.
fn opts(language: Option<Language>) -> InitOpts {
    InitOpts {
        language,
        force: false,
        no_seed: false,
        minimal: false,
        no_gitignore: false,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn init_rust_scaffolds_full_workspace() {
    let dir = tempfile::tempdir().unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    assert!(report.written.contains(&"AGENTS.md".to_owned()));
    assert!(report.written.contains(&"USE_RESTRICTIONS.md".to_owned()));
    assert!(report.written.contains(&"do-harness.toml".to_owned()));
    assert!(report.written.contains(&"plans/invariants.json".to_owned()));
    assert!(report.written.contains(&"scripts/check-loc.sh".to_owned()));
    assert!(
        report
            .written
            .contains(&".agents/skills/harness/SKILL.md".to_owned())
    );
    assert_eq!(report.seeded, 3);
    assert_eq!(report.language, Language::Rust);
    let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"rust\""));
    assert!(config.contains("[signal-sets]"));
    assert!(dir.path().join(".do-harness/agent_state.db").exists());
}

#[tokio::test(flavor = "current_thread")]
async fn init_agents_contract_embeds_pinned_installer() {
    let dir = tempfile::tempdir().unwrap();

    init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    let agents = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(agents.contains(&format!("--version v{}", env!("CARGO_PKG_VERSION"))));
    assert!(!agents.contains("{{VERSION}}"));
}

#[tokio::test(flavor = "current_thread")]
async fn init_generic_has_no_loc_script() {
    let dir = tempfile::tempdir().unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Generic)))
        .await
        .unwrap();

    assert!(!report.written.iter().any(|p| p == "scripts/check-loc.sh"));
    assert!(
        report
            .written
            .contains(&"scripts/check-tracker-drift.sh".to_owned()),
        "the language-agnostic drift check ships for every scaffold"
    );
    let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"generic\""));
    assert_eq!(report.seeded, 2);
}

#[tokio::test(flavor = "current_thread")]
async fn init_is_idempotent_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let opts = opts(Some(Language::Rust));
    init_workspace(dir.path(), &opts).await.unwrap();

    let report = init_workspace(dir.path(), &opts).await.unwrap();

    assert_eq!(report.written.len(), 0);
    assert!(report.skipped.contains(&"do-harness.toml".to_owned()));
    assert!(report.skipped.contains(&"USE_RESTRICTIONS.md".to_owned()));
}

#[tokio::test(flavor = "current_thread")]
async fn init_rust_scaffolds_skill_creator_and_evals() {
    let dir = tempfile::tempdir().unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    assert_eq!(report.skills, SKILLS.len());
    for spec in SKILLS {
        let name = spec.name;
        let skill_dir = dir.path().join(".agents/skills").join(name);
        assert!(skill_dir.join("SKILL.md").is_file(), "{name} SKILL.md");
        assert!(
            skill_dir.join("evals/evals.json").is_file(),
            "{name} evals.json"
        );
    }
    // Development-methodology skills are not scaffolded into adopters.
    for dev_skill in [
        "htn-planner",
        "spike-runner",
        "event-modeler",
        "skill-distiller",
    ] {
        assert!(
            !dir.path().join(".agents/skills").join(dev_skill).exists(),
            "{dev_skill} must not be scaffolded"
        );
    }
    let creator_dir = dir.path().join(".agents/skills/skill-creator");
    assert!(creator_dir.join("SKILL.md").is_file());
    assert!(creator_dir.join("scripts/init_skill.py").is_file());
    let quick = creator_dir.join("scripts/quick_validate.py");
    assert!(quick.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&quick).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "quick_validate.py must be executable");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn init_appends_gitignore_entries() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "# existing\n").unwrap();

    init_workspace(dir.path(), &opts(Some(Language::Generic)))
        .await
        .unwrap();

    let text = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(text.contains(".do-harness/"));
    assert!(text.contains(".agents/events/"));
    assert!(text.contains("# existing"));
}

/// The Rust pack's own build output must be ignored: its sensors run Cargo
/// (`target/`) and `cargo llvm-cov` (`lcov.info`), and untracked nonignored
/// files are sensor input identity, so an unignored build invalidates the
/// scaffolded verify run it was created by.
#[tokio::test(flavor = "current_thread")]
async fn init_rust_gitignore_covers_build_output() {
    let dir = tempfile::tempdir().unwrap();

    init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    let text = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(text.lines().any(|line| line == "/target"), "{text}");
    assert!(text.lines().any(|line| line == "/lcov.info"), "{text}");
}

/// Language-neutral packs keep a language-neutral ignore file: build entries
/// are Rust-only.
#[tokio::test(flavor = "current_thread")]
async fn init_generic_gitignore_stays_language_neutral() {
    let dir = tempfile::tempdir().unwrap();

    init_workspace(dir.path(), &opts(Some(Language::Generic)))
        .await
        .unwrap();

    let text = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(!text.contains("/target"), "{text}");
    assert!(!text.contains("/lcov.info"), "{text}");
}

#[tokio::test(flavor = "current_thread")]
async fn init_rust_writes_crate_when_manifest_absent() {
    let dir = tempfile::tempdir().unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    assert!(report.written.contains(&"Cargo.toml".to_owned()));
    assert!(report.written.contains(&"src/lib.rs".to_owned()));
    let manifest = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(manifest.contains("edition = \"2024\""));
    assert!(dir.path().join("src/lib.rs").is_file());
}

#[tokio::test(flavor = "current_thread")]
async fn init_rust_leaves_existing_crate_untouched() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"real\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), "pub fn real() {}\n").unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    assert!(report.skipped.contains(&"Cargo.toml".to_owned()));
    assert!(!report.written.iter().any(|p| p == "src/lib.rs"));
    let manifest = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("\"real\""),
        "existing manifest overwritten"
    );
    let lib = fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
    assert_eq!(lib, "pub fn real() {}\n", "existing lib overwritten");
}

#[tokio::test(flavor = "current_thread")]
async fn init_rust_never_overwrites_crate_even_with_force() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"real\"\n",
    )
    .unwrap();
    let mut force = opts(Some(Language::Rust));
    force.force = true;

    let report = init_workspace(dir.path(), &force).await.unwrap();

    assert!(report.skipped.contains(&"Cargo.toml".to_owned()));
    let manifest = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(
        manifest.contains("\"real\""),
        "force overwrote a real manifest"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn init_generic_writes_no_crate() {
    let dir = tempfile::tempdir().unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Generic)))
        .await
        .unwrap();

    assert!(!report.written.iter().any(|p| p == "Cargo.toml"));
    assert!(!dir.path().join("Cargo.toml").exists());
}

/// A `Cargo.toml` in the target directory selects the Rust pack by default.
#[tokio::test(flavor = "current_thread")]
async fn init_detects_rust_from_manifest() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"real\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), "pub fn real() {}\n").unwrap();

    let report = init_workspace(dir.path(), &opts(None)).await.unwrap();

    assert_eq!(report.language, Language::Rust);
    assert!(report.detected.iter().any(|f| f == "Cargo.toml"));
    assert!(report.detected.iter().any(|f| f == "Rust crate"));
    let manifest = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(manifest.contains("\"real\""));
}

/// A non-Rust project without `--language` gets the generic pack.
#[tokio::test(flavor = "current_thread")]
async fn init_detects_generic_from_package_json() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}\n").unwrap();

    let report = init_workspace(dir.path(), &opts(None)).await.unwrap();

    assert_eq!(report.language, Language::Generic);
    assert!(!dir.path().join("Cargo.toml").exists());
    let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"generic\""));
}

/// `--minimal` skips skill scaffolding but keeps the harness contract.
#[tokio::test(flavor = "current_thread")]
async fn init_minimal_skips_skills() {
    let dir = tempfile::tempdir().unwrap();
    let mut minimal = opts(Some(Language::Rust));
    minimal.minimal = true;

    let report = init_workspace(dir.path(), &minimal).await.unwrap();

    assert_eq!(report.skills, 0);
    assert!(!dir.path().join(".agents/skills").exists());
    assert!(dir.path().join("AGENTS.md").is_file());
    assert!(dir.path().join("USE_RESTRICTIONS.md").is_file());
}

/// `--no-gitignore` leaves the ignore file absent.
#[tokio::test(flavor = "current_thread")]
async fn init_no_gitignore_skips_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut no_gitignore = opts(Some(Language::Generic));
    no_gitignore.no_gitignore = true;

    init_workspace(dir.path(), &no_gitignore).await.unwrap();

    assert!(!dir.path().join(".gitignore").exists());
}

/// `--no-seed` leaves the state database without seeded invariants.
#[tokio::test(flavor = "current_thread")]
async fn init_no_seed_skips_seeding() {
    let dir = tempfile::tempdir().unwrap();
    let mut no_seed = opts(Some(Language::Rust));
    no_seed.no_seed = true;

    let report = init_workspace(dir.path(), &no_seed).await.unwrap();

    assert_eq!(report.seeded, 0);
    assert!(!dir.path().join(".do-harness/agent_state.db").exists());
}

/// The generated `feedback` set must name every sensor the generated
/// `pre-commit` hook runs.
///
/// The set is the fast loop and the hook is that same loop invoked at commit
/// time, so a set that omitted one of them would be a weaker check than the
/// commit it stands for — which is how `loc` came to be in the hook but not in
/// the set. The rust pack's `feedback` may hold more (`check`, `clippy`) than
/// the hook runs; it must not hold less.
#[tokio::test(flavor = "current_thread")]
async fn generated_feedback_set_covers_the_pre_commit_hook() {
    let dir = tempfile::tempdir().unwrap();
    init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();
    let path = dir.path().join("do-harness.toml");
    let cfg = crate::config::load(dir.path(), Some(&path)).await.unwrap();
    assert_fast_loop_covers_the_hook(&cfg.hooks.pre_commit, &cfg.signal_sets["feedback"]);
}

/// The repository's own policy holds the same rule as the generated one, so the
/// two cannot drift apart: this repository's hook and set are what its
/// contributors actually run.
#[tokio::test(flavor = "current_thread")]
async fn repository_feedback_set_covers_its_pre_commit_hook() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("do-harness.toml");
    let cfg = crate::config::load(&root, Some(&path)).await.unwrap();
    assert_fast_loop_covers_the_hook(&cfg.hooks.pre_commit, &cfg.signal_sets["feedback"]);
}

#[tokio::test(flavor = "current_thread")]
async fn init_scaffolds_use_restrictions_policy_across_all_languages() {
    for lang in [
        Language::Rust,
        Language::Generic,
        Language::Web,
        Language::Node,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let report = init_workspace(dir.path(), &opts(Some(lang))).await.unwrap();

        assert!(
            report.written.contains(&"USE_RESTRICTIONS.md".to_owned()),
            "USE_RESTRICTIONS.md must be written for language {lang:?}"
        );
        let path = dir.path().join("USE_RESTRICTIONS.md");
        assert!(path.is_file(), "USE_RESTRICTIONS.md exists for {lang:?}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn init_use_restrictions_preexisting_is_kept_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let custom_policy = "# Custom Adopter Policy\nDo not break stuff.\n";
    fs::write(dir.path().join("USE_RESTRICTIONS.md"), custom_policy).unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    assert!(report.skipped.contains(&"USE_RESTRICTIONS.md".to_owned()));
    assert!(!report.written.contains(&"USE_RESTRICTIONS.md".to_owned()));
    let policy = fs::read_to_string(dir.path().join("USE_RESTRICTIONS.md")).unwrap();
    assert_eq!(policy, custom_policy);
}

#[tokio::test(flavor = "current_thread")]
async fn init_use_restrictions_preexisting_is_overwritten_with_force() {
    let dir = tempfile::tempdir().unwrap();
    let custom_policy = "# Custom Adopter Policy\nDo not break stuff.\n";
    fs::write(dir.path().join("USE_RESTRICTIONS.md"), custom_policy).unwrap();

    let mut force_opts = opts(Some(Language::Rust));
    force_opts.force = true;

    let report = init_workspace(dir.path(), &force_opts).await.unwrap();

    assert!(report.written.contains(&"USE_RESTRICTIONS.md".to_owned()));
    let policy = fs::read_to_string(dir.path().join("USE_RESTRICTIONS.md")).unwrap();
    assert_ne!(policy, custom_policy);
}

fn assert_fast_loop_covers_the_hook(pre_commit: &[String], feedback: &[String]) {
    assert!(!pre_commit.is_empty(), "the hook runs something");
    for sensor in pre_commit {
        assert!(
            feedback.contains(sensor),
            "the fast loop must include {sensor}, which the pre-commit hook runs: {feedback:?}"
        );
    }
}

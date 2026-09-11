//! Unit tests for `do-harness init` (`init.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

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
async fn init_generic_has_no_loc_script() {
    let dir = tempfile::tempdir().unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Generic)))
        .await
        .unwrap();

    assert!(!report.written.iter().any(|p| p == "scripts/check-loc.sh"));
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

    assert!(report.written.is_empty());
    assert!(report.skipped.contains(&"do-harness.toml".to_owned()));
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

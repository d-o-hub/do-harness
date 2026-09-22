//! Web-pack scaffolding tests, split out of `init/tests.rs`.

use super::super::*;
use super::opts;

#[tokio::test(flavor = "current_thread")]
async fn init_web_scaffolds_sensors_library_and_runners() {
    let dir = tempfile::tempdir().unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Web)))
        .await
        .unwrap();

    assert_eq!(report.language, Language::Web);
    let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"web\""));
    assert!(config.contains("name = \"viewport-ux\""));
    assert!(config.contains("name = \"a11y\""));
    assert!(config.contains("name = \"console\""));
    assert!(config.contains("name = \"perf\""));
    assert!(config.contains("name = \"visual\""));
    assert!(config.contains("name = \"i18n\""));
    // Sensor runners + the bundled audit library land under scripts/.
    for path in [
        "scripts/viewport-audit.mjs",
        "scripts/a11y-audit.mjs",
        "scripts/console-audit.mjs",
        "scripts/perf-audit.mjs",
        "scripts/visual-audit.mjs",
        "scripts/i18n-audit.mjs",
        "scripts/web-ui/audit.test.mjs",
        "scripts/web-ui/audit.browser.test.mjs",
        "scripts/web-ui/lib/audit.mjs",
        "scripts/web-ui/lib/annotate.mjs",
        "scripts/web-ui/lib/page-probe.mjs",
        "scripts/web-ui/lib/geometry.mjs",
        "scripts/web-ui/lib/contrast.mjs",
        "scripts/web-ui/lib/console-audit.mjs",
        "scripts/web-ui/lib/a11y-audit.mjs",
        "scripts/web-ui/lib/perf-audit.mjs",
        "scripts/web-ui/lib/visual-audit.mjs",
        "scripts/web-ui/lib/i18n-audit.mjs",
        "scripts/web-ui/fixtures/clean.html",
        "scripts/web-ui/fixtures/overlap.html",
        "scripts/web-ui/fixtures/overlap-descendant.html",
        "scripts/web-ui/fixtures/contrast.html",
        "scripts/web-ui/fixtures/app-shell.html",
    ] {
        assert!(dir.path().join(path).exists(), "missing {path}");
    }
    // The headless unit tests must pass as shipped (no browser needed).
    let status = std::process::Command::new("node")
        .arg("--test")
        .arg(dir.path().join("scripts/web-ui/audit.test.mjs"))
        .status()
        .expect("node is available in CI and dev environments");
    assert!(status.success(), "shipped web-ui unit tests must pass");
}

/// The vendored web-ui templates must stay identical to `integrations/web-ui`
/// (the dev-side source of truth): scaffolded adopters would otherwise run
/// stale audit code. Update both trees together when the library changes.
#[test]
fn web_ui_templates_match_integrations() {
    let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("crate lives at crates/<name>");
    for relative in [
        "audit.test.mjs",
        "audit.browser.test.mjs",
        "lib/audit.mjs",
        "lib/annotate.mjs",
        "lib/page-probe.mjs",
        "lib/geometry.mjs",
        "lib/contrast.mjs",
        "lib/console-audit.mjs",
        "lib/a11y-audit.mjs",
        "lib/perf-audit.mjs",
        "lib/visual-audit.mjs",
        "lib/i18n-audit.mjs",
        "fixtures/clean.html",
        "fixtures/overlap.html",
        "fixtures/overlap-descendant.html",
        "fixtures/contrast.html",
        "fixtures/app-shell.html",
    ] {
        let source = workspace.join("integrations/web-ui").join(relative);
        let template = workspace
            .join("crates/do-harness/templates/scripts/web-ui")
            .join(relative);
        let source_body =
            fs::read_to_string(&source).unwrap_or_else(|_| panic!("missing {relative}"));
        let template_body =
            fs::read_to_string(&template).unwrap_or_else(|_| panic!("unvendored {relative}"));
        assert_eq!(
            source_body, template_body,
            "template drift: {relative} — copy integrations/web-ui/{relative} over the template"
        );
    }
}

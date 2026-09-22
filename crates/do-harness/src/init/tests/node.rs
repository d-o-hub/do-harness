//! Node-pack scaffolding tests, split out of `init/tests.rs`.

use super::super::*;
use super::opts;

#[tokio::test(flavor = "current_thread")]
async fn init_node_pnpm_turbo_workspace_first_run_is_green() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("pnpm-workspace.yaml"),
        "packages:\n  - 'apps/*'\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("turbo.json"),
        r#"{"tasks": {"typecheck": {}, "lint": {}, "test": {}, "build": {}}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("package.json"),
        // Script bodies must succeed on every platform: `true` is a POSIX
        // shell builtin with no Windows executable, so `npm run typecheck`
        // would fail there and the baseline would read Red. A node no-op is
        // portable.
        r#"{"name": "ws", "scripts": {"typecheck": "node -e \"\"", "lint": "node -e \"\"", "test": "node -e \"\"", "build": "node -e \"\""}}"#,
    )
    .unwrap();

    let mut report = init_workspace(dir.path(), &opts(Some(Language::Node)))
        .await
        .unwrap();

    assert_eq!(report.language, Language::Node);
    let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    assert!(config.contains("language = \"node\""));
    assert!(config.contains("typecheck"));
    assert!(config.contains("lint"));
    assert!(config.contains("test"));
    assert!(config.contains("build"));
    assert!(config.contains("**/*.ts"));
    assert!(config.contains("feedback = [\"typecheck\", \"lint\"]"));
    assert!(config.contains("verification = [\"typecheck\", \"lint\", \"test\"]"));
    assert!(config.contains("release = [\"typecheck\", \"lint\", \"test\", \"build\"]"));

    run_baseline(dir.path(), &mut report).await.unwrap();
    assert_eq!(
        report.baseline.as_ref().map(|b| b.state),
        Some(BaselineState::Green)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn init_node_probes_only_proven_signals() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name": "app", "scripts": {"typecheck": "true"}}"#,
    )
    .unwrap();

    let report = init_workspace(dir.path(), &opts(Some(Language::Node)))
        .await
        .unwrap();

    assert_eq!(report.language, Language::Node);
    let config = fs::read_to_string(dir.path().join("do-harness.toml")).unwrap();
    assert!(config.contains("name = \"typecheck\""));
    assert!(!config.contains("name = \"lint\""));
    assert!(!config.contains("name = \"test\""));
    assert!(!config.contains("name = \"build\""));
    assert!(config.contains("feedback = [\"typecheck\"]"));
    assert!(config.contains("verification = [\"typecheck\"]"));
    assert!(config.contains("release = [\"typecheck\"]"));
}

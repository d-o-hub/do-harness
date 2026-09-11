#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

/// Writes a config file into a tempdir and loads it explicitly.
#[tokio::test(flavor = "current_thread")]
async fn parses_valid_config() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    let text = r#"
            language = "rust"
            [hooks]
            pre-commit = ["fmt", "loc"]
            pre-push = []
            [[sensors]]
            name = "fmt"
            argv = ["cargo", "fmt", "--all", "--", "--check"]
            [[sensors]]
            name = "check"
            argv = ["cargo", "check", "--workspace"]
        "#;
    std::fs::write(&path, text).expect("write config");
    let cfg = load(dir.path(), Some(&path)).await.expect("load config");
    assert_eq!(cfg.language.as_deref(), Some("rust"));
    assert_eq!(
        cfg.hooks.pre_commit,
        vec!["fmt".to_owned(), "loc".to_owned()]
    );
    assert!(cfg.hooks.pre_push.is_empty());
    assert_eq!(cfg.sensors.len(), 2);
    assert_eq!(
        cfg.sensors[0].argv,
        vec!["cargo", "fmt", "--all", "--", "--check"]
    );
}

/// Unknown top-level keys are rejected by `deny_unknown_fields`.
#[tokio::test(flavor = "current_thread")]
async fn rejects_unknown_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    std::fs::write(&path, "bogus_key = 1\n").expect("write config");
    let err = load(dir.path(), Some(&path))
        .await
        .expect_err("load must fail");
    assert!(format!("{err:#}").contains("bogus_key"));
}

/// Parses new per-sensor fields: retry, timeout, `allow_failure`, `transient_exit_codes`.
#[tokio::test(flavor = "current_thread")]
async fn parses_transient_failure_sensor_options() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    let text = r#"
            [[sensors]]
            name = "links"
            argv = ["bash", "check-links.sh"]
            retry = 3
            timeout = 10
            allow_failure = true
            transient_exit_codes = [75, 429]
        "#;
    std::fs::write(&path, text).expect("write config");
    let cfg = load(dir.path(), Some(&path)).await.expect("load config");
    assert_eq!(cfg.sensors.len(), 1);
    let sensor = &cfg.sensors[0];
    assert_eq!(sensor.name, "links");
    assert_eq!(sensor.retry, Some(3));
    assert_eq!(sensor.timeout, Some(10));
    assert!(sensor.allow_failure);
    assert_eq!(sensor.transient_exit_codes, vec![75, 429]);
}

/// Unknown fields in [[sensors]] are rejected by `deny_unknown_fields`.
#[tokio::test(flavor = "current_thread")]
async fn rejects_unknown_sensor_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    let text = r#"
            [[sensors]]
            name = "fmt"
            argv = ["cargo", "fmt"]
            unknown_sensor_option = true
        "#;
    std::fs::write(&path, text).expect("write config");
    let err = load(dir.path(), Some(&path))
        .await
        .expect_err("load must fail");
    assert!(format!("{err:#}").contains("unknown_sensor_option"));
}

/// A missing default config falls back to the built-in Rust pack.
#[tokio::test(flavor = "current_thread")]
async fn missing_file_returns_rust_default() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = load(dir.path(), None).await.expect("load default");
    assert_eq!(cfg.language, None);
    assert_eq!(
        cfg.hooks.pre_commit,
        vec!["fmt".to_owned(), "loc".to_owned()]
    );
    assert!(cfg.hooks.pre_push.is_empty());
    assert_eq!(cfg.sensors.len(), 8);
    assert_eq!(cfg.sensor_names().len(), 8);
    assert_eq!(cfg.effective_sensors().len(), 8);
}

/// An unknown language pack identifier is rejected at load time.
#[tokio::test(flavor = "current_thread")]
async fn rejects_unknown_language_pack() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    std::fs::write(&path, "language = \"python\"\n").expect("write config");
    let err = load(dir.path(), Some(&path))
        .await
        .expect_err("load must fail");
    assert!(format!("{err:#}").contains("unsupported language pack 'python'"));
}

/// The generic pack with no sensors yields an empty sensor list.
#[tokio::test(flavor = "current_thread")]
async fn generic_language_yields_no_effective_sensors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    std::fs::write(&path, "language = \"generic\"\n").expect("write config");
    let cfg = load(dir.path(), Some(&path)).await.expect("load config");
    assert!(cfg.effective_sensors().is_empty());
    assert!(cfg.sensor_names().is_empty());
}

/// The rust pack with no sensors falls back to the built-in Rust sensors.
#[tokio::test(flavor = "current_thread")]
async fn rust_language_without_sensors_uses_builtin_pack() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    std::fs::write(&path, "language = \"rust\"\n").expect("write config");
    let cfg = load(dir.path(), Some(&path)).await.expect("load config");
    assert_eq!(cfg.effective_sensors().len(), 8);
    assert!(cfg.sensor_names().contains(&"clippy".to_owned()));
}

/// Parses `[signal-sets]` and `when-changed` sensor patterns.
#[tokio::test(flavor = "current_thread")]
async fn parses_signal_sets_and_when_changed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    let text = r#"
            [signal-sets]
            feedback = ["fmt", "check"]
            verification = ["fmt", "check", "test"]

            [[sensors]]
            name = "fmt"
            argv = ["cargo", "fmt"]
            when-changed = ["**/*.rs", "Cargo.toml"]

            [[sensors]]
            name = "check"
            argv = ["cargo", "check"]

            [[sensors]]
            name = "test"
            argv = ["cargo", "test"]
        "#;
    std::fs::write(&path, text).expect("write config");
    let cfg = load(dir.path(), Some(&path)).await.expect("load config");
    assert_eq!(
        cfg.signal_sets.get("feedback"),
        Some(&vec!["fmt".to_owned(), "check".to_owned()])
    );
    assert_eq!(cfg.sensors[0].when_changed, vec!["**/*.rs", "Cargo.toml"]);
    assert!(cfg.sensors[1].when_changed.is_empty());
}

/// A signal set referencing an unknown sensor fails config validation.
#[tokio::test(flavor = "current_thread")]
async fn rejects_unknown_sensor_in_signal_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    let text = r#"
            [signal-sets]
            feedback = ["ghost"]

            [[sensors]]
            name = "real"
            argv = ["true"]
        "#;
    std::fs::write(&path, text).expect("write config");
    let err = load(dir.path(), Some(&path))
        .await
        .expect_err("load must fail");
    assert!(format!("{err:#}").contains("ghost"));
}

/// Duplicate sensor names inside a signal set are rejected.
#[tokio::test(flavor = "current_thread")]
async fn rejects_duplicate_sensor_in_signal_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    let text = r#"
            [signal-sets]
            feedback = ["real", "real"]

            [[sensors]]
            name = "real"
            argv = ["true"]
        "#;
    std::fs::write(&path, text).expect("write config");
    let err = load(dir.path(), Some(&path))
        .await
        .expect_err("load must fail");
    assert!(format!("{err:#}").contains("duplicate"));
}

/// Signal-set names are restricted to path-safe characters.
#[tokio::test(flavor = "current_thread")]
async fn rejects_invalid_signal_set_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    let text = r#"
            [signal-sets]
            "evil/set" = ["real"]

            [[sensors]]
            name = "real"
            argv = ["true"]
        "#;
    std::fs::write(&path, text).expect("write config");
    let err = load(dir.path(), Some(&path))
        .await
        .expect_err("load must fail");
    assert!(format!("{err:#}").contains("invalid signal set name"));
}

/// An invalid `when-changed` glob fails at config load, not at verify time.
#[tokio::test(flavor = "current_thread")]
async fn rejects_invalid_when_changed_glob() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("do-harness.toml");
    let text = r#"
            [[sensors]]
            name = "real"
            argv = ["true"]
            when-changed = ["[unclosed"]
        "#;
    std::fs::write(&path, text).expect("write config");
    let err = load(dir.path(), Some(&path))
        .await
        .expect_err("load must fail");
    assert!(format!("{err:#}").contains("when-changed"));
}

/// The built-in default ships feedback/verification/release signal sets.
#[test]
fn rust_default_ships_signal_sets() {
    let cfg = rust_default();
    assert_eq!(
        cfg.signal_sets.get("feedback"),
        Some(&vec![
            "fmt".to_owned(),
            "check".to_owned(),
            "clippy".to_owned()
        ])
    );
    assert_eq!(cfg.signal_sets.get("verification").unwrap().len(), 8);
    assert_eq!(cfg.signal_sets.get("release").unwrap().len(), 8);
    cfg.validate().expect("built-in default must validate");
}

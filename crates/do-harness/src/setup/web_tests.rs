//! Unit tests for `setup/web` pure logic (detection is covered by
//! `init::node_manifest` tests; the binary path by the integration tests).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn parse_version_reads_numeric_triple() {
    assert_eq!(parse_version("1.40"), Some((1, 40, 0)));
    assert_eq!(parse_version("1.40.1"), Some((1, 40, 1)));
    assert_eq!(parse_version("v11.0.0"), Some((11, 0, 0)));
    assert_eq!(parse_version("11.0.0-beta.2"), Some((11, 0, 0)));
    assert_eq!(parse_version("4.8.0"), Some((4, 8, 0)));
    assert_eq!(parse_version("not-a-version"), None);
    assert_eq!(parse_version(""), None);
}

#[test]
fn version_at_least_compares_floors() {
    assert!(version_at_least("1.40.0", "1.40"));
    assert!(version_at_least("1.40.1", "1.40"));
    assert!(version_at_least("2.0.0", "1.40"));
    assert!(version_at_least("11.0.0", "11.0"));
    assert!(!version_at_least("1.39.9", "1.40"));
    assert!(!version_at_least("4.7.0", "4.8"));
    assert!(!version_at_least("garbage", "1.40"));
}

#[test]
fn installed_version_reads_node_modules_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert_eq!(installed_version(root, "playwright"), None);

    std::fs::create_dir_all(root.join("node_modules/playwright")).unwrap();
    std::fs::write(
        root.join("node_modules/playwright/package.json"),
        r#"{"name": "playwright", "version": "1.41.0"}"#,
    )
    .unwrap();
    assert_eq!(
        installed_version(root, "playwright"),
        Some("1.41.0".to_owned())
    );
}

#[test]
fn missing_peer_deps_checks_floors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Nothing installed: every peer dep is missing.
    assert_eq!(
        missing_peer_deps(root),
        vec!["playwright", "@axe-core/playwright", "lighthouse"]
    );

    let stub = |name: &str, version: &str| {
        std::fs::create_dir_all(root.join("node_modules").join(name)).unwrap();
        std::fs::write(
            root.join("node_modules").join(name).join("package.json"),
            format!(r#"{{"name": "{name}", "version": "{version}"}}"#),
        )
        .unwrap();
    };
    stub("playwright", "1.40.0");
    stub("@axe-core/playwright", "4.8.0");
    stub("lighthouse", "11.0.0");
    assert_eq!(missing_peer_deps(root), [] as [&str; 0]);

    // Below floor => treated as missing.
    stub("lighthouse", "10.9.0");
    assert_eq!(missing_peer_deps(root), vec!["lighthouse"]);
}

#[test]
fn install_command_maps_each_package_manager() {
    assert_eq!(
        install_command(PackageManager::Npm),
        ("npm", vec!["install", "-D"])
    );
    assert_eq!(
        install_command(PackageManager::Pnpm),
        ("pnpm", vec!["add", "-D"])
    );
    assert_eq!(
        install_command(PackageManager::Yarn),
        ("yarn", vec!["add", "-D"])
    );
    assert_eq!(
        install_command(PackageManager::Bun),
        ("bun", vec!["add", "-d"])
    );
}

#[test]
fn patch_config_absent_env_appends_block() {
    let text = "language = \"web\"\n";
    let (patched, additions) = patch_config_text(text);
    assert_ne!(patched, text);
    assert!(patched.contains("[env]"));
    assert!(patched.contains("WEB_AUDIT_ROUTES = \"/\""));
    assert!(patched.contains("WEB_AUDIT_BASE_URL = \"http://127.0.0.1:3000\""));
    assert_eq!(additions.len(), 2);
    // The patched text must still parse and carry the env keys.
    let table: toml::Table = patched.parse().unwrap();
    assert_eq!(table["env"]["WEB_AUDIT_ROUTES"].as_str(), Some("/"));
}

#[test]
fn patch_config_preserves_existing_env_values() {
    let text = "# header\nlanguage = \"web\"\n[env]\nWEB_AUDIT_ROUTES = \"/app\"\n";
    let (patched, additions) = patch_config_text(text);
    assert!(patched.contains("WEB_AUDIT_ROUTES = \"/app\""));
    assert!(patched.contains("WEB_AUDIT_BASE_URL = \"http://127.0.0.1:3000\""));
    assert_eq!(additions.len(), 1);
    let table: toml::Table = patched.parse().unwrap();
    assert_eq!(table["env"]["WEB_AUDIT_ROUTES"].as_str(), Some("/app"));
}

#[test]
fn patch_config_noop_when_defaults_present() {
    let text = "# header\nlanguage = \"web\"\n[env]\n\
        WEB_AUDIT_ROUTES = \"/\"\n\
        WEB_AUDIT_BASE_URL = \"http://127.0.0.1:3000\"\n";
    let (patched, additions) = patch_config_text(text);
    assert_eq!(patched, text);
    assert!(additions.is_empty());
}

#[test]
fn patch_config_keeps_user_env_table_untouched_when_complex() {
    let text = "# header\nlanguage = \"web\"\n[env]\nWEB_AUDIT_ROUTES = \"/\"\nworker.count = 2\n";
    let (patched, additions) = patch_config_text(text);
    assert_eq!(patched, text);
    assert!(additions.is_empty());
}

#[test]
fn render_env_table_sorts_keys() {
    let mut vars = BTreeMap::new();
    vars.insert("B".to_owned(), "2".to_owned());
    vars.insert("A".to_owned(), "1".to_owned());
    let rendered = render_env_table(&vars);
    assert_eq!(rendered, "[env]\nA = \"1\"\nB = \"2\"\n");
}

#[test]
fn find_env_table_lines_locates_top_level_table() {
    let text = "language = \"web\"\n[hooks]\npre-commit = []\n[env]\nA = \"1\"\nB = \"2\"\n";
    let (start, end) = find_env_table_lines(text).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[start], "[env]");
    assert_eq!(lines[start + 1], "A = \"1\"");
    assert_eq!(end, lines.len());
}

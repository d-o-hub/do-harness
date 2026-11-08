//! Pure, non-executing analysis of a repository's Node metadata.
//!
//! No tools are probed or executed here; the analysis reads manifests and
//! marker files on disk. Both language selection (`detect::inspect`,
//! `inspect::inspect_workspace`) and candidate probing
//! (`node::probe_node`) consume the same analysis so `init` and
//! `init --inspect` agree without running package scripts or installers.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// What the repository's Node metadata proves, independent of tool presence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct NodeManifestAnalysis {
    /// Whether `package.json` exists in `root`.
    pub has_package_json: bool,
    /// Parse error message when package.json exists but is malformed.
    pub malformed: Option<String>,
    /// Declared `scripts` map: script name -> raw body string.
    pub scripts: BTreeMap<String, String>,
    /// Turbo tasks declared in `turbo.json` (`tasks` or legacy `pipeline`).
    pub turbo_tasks: BTreeSet<String>,
    /// Whether the matching dependency is declared anywhere in package.json.
    pub dep_typescript: bool,
    /// Whether the matching dependency is declared anywhere in package.json.
    pub dep_eslint: bool,
    /// Whether the matching dependency is declared anywhere in package.json.
    pub dep_vitest: bool,
    /// Whether the matching dependency is declared anywhere in package.json.
    pub dep_jest: bool,
    /// Whether a `tsconfig.json`/`tsconfig.base.json` exists.
    pub has_tsconfig: bool,
    /// Whether an `ESLint` configuration file exists.
    pub has_eslint_config: bool,
}

impl NodeManifestAnalysis {
    /// Signal names this repository's metadata proves, in pack order.
    pub fn supported_commands(&self) -> Vec<&'static str> {
        let mut supported = Vec::new();
        if self.has_script("typecheck")
            || self.has_turbo_task("typecheck")
            || self.has_tsconfig
            || self.dep_typescript
        {
            supported.push("typecheck");
        }
        if self.has_script("lint")
            || self.has_turbo_task("lint")
            || self.has_eslint_config
            || self.dep_eslint
        {
            supported.push("lint");
        }
        // The npm scaffold's dummy `test` body is not a supported command: it
        // proves nothing when run.
        let real_test = self
            .get_script("test")
            .is_some_and(|body| !body.contains("no test specified"));
        if real_test || self.has_turbo_task("test") || self.dep_vitest || self.dep_jest {
            supported.push("test");
        }
        if self.has_script("build") || self.has_turbo_task("build") {
            supported.push("build");
        }
        supported
    }

    /// Whether the manifest declares `name` as a script.
    pub fn has_script(&self, name: &str) -> bool {
        self.scripts.contains_key(name)
    }

    /// The raw body of script `name`, when declared.
    pub fn get_script(&self, name: &str) -> Option<&str> {
        self.scripts.get(name).map(String::as_str)
    }

    /// Whether `turbo.json` declares `name` as a task.
    pub fn has_turbo_task(&self, name: &str) -> bool {
        self.turbo_tasks.contains(name)
    }

    /// Whether the named dependency is declared in package.json.
    pub fn has_dep(&self, name: &str) -> bool {
        match name {
            "typescript" => self.dep_typescript,
            "eslint" => self.dep_eslint,
            "vitest" => self.dep_vitest,
            "jest" => self.dep_jest,
            _ => false,
        }
    }
}

/// Analyzes `root`'s Node metadata without probing or executing tools.
#[must_use]
pub fn analyze_node_manifest(root: &Path) -> NodeManifestAnalysis {
    let mut analysis = NodeManifestAnalysis::default();
    let pkg_path = root.join("package.json");
    if !pkg_path.exists() {
        return analysis;
    }
    analysis.has_package_json = true;
    let Ok(text) = fs::read_to_string(&pkg_path) else {
        analysis.malformed = Some(format!("could not read {}", pkg_path.display()));
        return analysis;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        analysis.malformed = Some("expected a JSON object".to_owned());
        return analysis;
    };
    if let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) {
        for (name, body) in scripts {
            if let Some(body) = body.as_str() {
                analysis.scripts.insert(name.clone(), body.to_owned());
            }
        }
    }
    let has_dep = |value: &serde_json::Value, key: &str| -> bool {
        ["dependencies", "devDependencies", "peerDependencies"]
            .iter()
            .any(|section| {
                value
                    .get(*section)
                    .and_then(|d| d.as_object())
                    .is_some_and(|obj| obj.contains_key(key))
            })
    };
    analysis.dep_typescript = has_dep(&value, "typescript");
    analysis.dep_eslint = has_dep(&value, "eslint");
    analysis.dep_vitest = has_dep(&value, "vitest");
    analysis.dep_jest = has_dep(&value, "jest");

    analysis.has_tsconfig =
        root.join("tsconfig.json").exists() || root.join("tsconfig.base.json").exists();
    analysis.has_eslint_config = [
        ".eslintrc",
        ".eslintrc.json",
        ".eslintrc.js",
        ".eslintrc.cjs",
        "eslint.config.js",
        "eslint.config.mjs",
        "eslint.config.cjs",
    ]
    .iter()
    .any(|name| root.join(name).exists());

    if let Ok(turbo_text) = fs::read_to_string(root.join("turbo.json")) {
        if let Ok(turbo) = serde_json::from_str::<serde_json::Value>(&turbo_text) {
            if let Some(tasks) = turbo
                .get("tasks")
                .or_else(|| turbo.get("pipeline"))
                .and_then(|t| t.as_object())
            {
                analysis.turbo_tasks.extend(tasks.keys().cloned());
            }
        }
    }
    analysis
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn missing_package_json_is_not_a_node_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let analysis = analyze_node_manifest(dir.path());
        assert!(!analysis.has_package_json);
        assert!(analysis.malformed.is_none());
        assert_eq!(analysis.supported_commands(), Vec::<&'static str>::new());
    }

    #[test]
    fn malformed_package_json_reports_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{oops").unwrap();
        let analysis = analyze_node_manifest(dir.path());
        assert!(analysis.has_package_json);
        assert!(analysis.malformed.is_some());
    }

    #[test]
    fn dummy_test_script_is_not_supported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"test": "echo \"Error: no test specified\" && exit 1"}}"#,
        )
        .unwrap();
        let analysis = analyze_node_manifest(dir.path());
        assert!(!analysis.supported_commands().contains(&"test"));
        assert_eq!(analysis.supported_commands(), Vec::<&'static str>::new());
    }

    #[test]
    fn real_scripts_and_dependencies_prove_commands() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{
                "scripts": {"test": "node --test", "build": "tsc"},
                "devDependencies": {"typescript": "^5.0.0", "eslint": "^9.0.0"}
            }"#,
        )
        .unwrap();
        let analysis = analyze_node_manifest(dir.path());
        assert_eq!(
            analysis.supported_commands(),
            vec!["typecheck", "lint", "test", "build"]
        );
    }

    #[test]
    fn turbo_tasks_prove_commands() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name": "ws"}"#).unwrap();
        fs::write(
            dir.path().join("turbo.json"),
            r#"{"tasks": {"typecheck": {}, "lint": {}}}"#,
        )
        .unwrap();
        let analysis = analyze_node_manifest(dir.path());
        assert_eq!(analysis.supported_commands(), vec!["typecheck", "lint"]);
    }
}

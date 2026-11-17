//! Pure, non-executing analysis of a repository's Python metadata.
//!
//! No tools are probed or executed here; the analysis reads manifests and
//! marker files on disk. Both language selection (`detect::inspect`,
//! `inspect::inspect_workspace`) and candidate probing
//! (`python::probe_python`) consume the same analysis so `init` and
//! `init --inspect` agree without running the toolchain.

use std::fs;
use std::path::Path;

/// Python project markers that select the Python pack.
pub const PYTHON_MARKERS: &[&str] = &[
    "pyproject.toml",
    "requirements.txt",
    "setup.py",
    "setup.cfg",
    "tox.ini",
];

/// What the repository's Python metadata proves, independent of tool presence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PythonManifestAnalysis {
    /// Whether any Python project marker exists in `root`.
    pub has_python_markers: bool,
    /// Whether a type checker is configured (mypy/pyright). This is the gate
    /// for the optional typecheck sensor: without configuration the sensor is
    /// not proven, so it is never included even when a checker binary exists.
    pub type_checker_configured: bool,
    /// The configured type-checker tool name when known: "mypy" or "pyright".
    pub type_checker: Option<&'static str>,
}

impl PythonManifestAnalysis {
    /// The configured type-checker tool name, if any.
    #[must_use]
    pub fn type_checker_tool(&self) -> Option<&'static str> {
        self.type_checker
    }
}

/// Analyzes `root`'s Python metadata without probing or executing tools.
#[must_use]
pub fn analyze_python_manifest(root: &Path) -> PythonManifestAnalysis {
    let mut analysis = PythonManifestAnalysis::default();
    if !PYTHON_MARKERS.iter().any(|name| root.join(name).exists()) {
        return analysis;
    }
    analysis.has_python_markers = true;

    // pyproject.toml [tool.mypy] / [tool.pyright]
    if let Ok(text) = fs::read_to_string(root.join("pyproject.toml")) {
        if let Ok(value) = toml::from_str::<toml::Value>(&text) {
            if let Some(tool) = value.get("tool") {
                if tool.get("mypy").is_some() {
                    analysis.type_checker_configured = true;
                    analysis.type_checker = Some("mypy");
                } else if tool.get("pyright").is_some() {
                    analysis.type_checker_configured = true;
                    analysis.type_checker = Some("pyright");
                }
            }
        }
    }

    // mypy.ini / .mypy.ini
    if !analysis.type_checker_configured {
        for name in ["mypy.ini", ".mypy.ini"] {
            if root.join(name).exists() {
                analysis.type_checker_configured = true;
                analysis.type_checker = Some("mypy");
                break;
            }
        }
    }

    // setup.cfg [mypy]
    if !analysis.type_checker_configured {
        if let Ok(text) = fs::read_to_string(root.join("setup.cfg")) {
            if text.lines().any(|line| line.trim() == "[mypy]") {
                analysis.type_checker_configured = true;
                analysis.type_checker = Some("mypy");
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
    fn missing_markers_is_not_a_python_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let analysis = analyze_python_manifest(dir.path());
        assert!(!analysis.has_python_markers);
        assert!(!analysis.type_checker_configured);
        assert_eq!(analysis.type_checker_tool(), None);
    }

    #[test]
    fn pyproject_tool_mypy_configured_type_checker() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n\n[tool.mypy]\nstrict = true\n",
        )
        .unwrap();
        let analysis = analyze_python_manifest(dir.path());
        assert!(analysis.has_python_markers);
        assert!(analysis.type_checker_configured);
        assert_eq!(analysis.type_checker_tool(), Some("mypy"));
    }

    #[test]
    fn pyproject_tool_pyright_configured_type_checker() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n\n[tool.pyright]\ninclude = [\"src\"]\n",
        )
        .unwrap();
        let analysis = analyze_python_manifest(dir.path());
        assert!(analysis.type_checker_configured);
        assert_eq!(analysis.type_checker_tool(), Some("pyright"));
    }

    #[test]
    fn mypy_ini_configured_type_checker() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("mypy.ini"), "[mypy]\n").unwrap();
        let analysis = analyze_python_manifest(dir.path());
        assert!(analysis.type_checker_configured);
        assert_eq!(analysis.type_checker_tool(), Some("mypy"));
    }

    #[test]
    fn setup_cfg_mypy_section_configured_type_checker() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("setup.cfg"),
            "[metadata]\nname = x\n\n[mypy]\n",
        )
        .unwrap();
        let analysis = analyze_python_manifest(dir.path());
        assert!(analysis.type_checker_configured);
        assert_eq!(analysis.type_checker_tool(), Some("mypy"));
    }

    #[test]
    fn requirements_txt_marker_without_type_checker() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("requirements.txt"), "requests\n").unwrap();
        let analysis = analyze_python_manifest(dir.path());
        assert!(analysis.has_python_markers);
        assert!(!analysis.type_checker_configured);
    }
}

//! Pure, non-executing analysis of a repository's Go metadata.
//!
//! The Go pack has a single decisive marker (`go.mod`); unlike Node, there is
//! no script manifest to prove commands from. This module exists to keep the
//! language-selection and probing paths symmetric with the other packs and to
//! give future Go-specific analysis (module layout, build tags) a home.

use std::path::Path;

/// What the repository's Go metadata proves, independent of tool presence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GoManifestAnalysis {
    /// Whether `go.mod` exists in `root`.
    pub has_go_mod: bool,
}

/// Analyzes `root`'s Go metadata without probing or executing tools.
#[must_use]
pub fn analyze_go_manifest(root: &Path) -> GoManifestAnalysis {
    GoManifestAnalysis {
        has_go_mod: root.join("go.mod").exists(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn go_mod_marker_selects_go() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!analyze_go_manifest(dir.path()).has_go_mod);
        std::fs::write(
            dir.path().join("go.mod"),
            "module example.com/x\n\ngo 1.22\n",
        )
        .unwrap();
        assert!(analyze_go_manifest(dir.path()).has_go_mod);
    }
}

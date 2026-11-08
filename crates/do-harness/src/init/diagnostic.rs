//! Structured, actionable onboarding diagnostics for `do-harness init`.

use serde::{Deserialize, Serialize};

/// Diagnostic code constants.
pub mod code {
    /// `package.json` exists but cannot be parsed as JSON.
    pub const INVALID_PACKAGE_JSON: &str = "invalid_package_json";
    /// `package.json` proves no supported Node command.
    pub const NO_SUPPORTED_NODE_COMMANDS: &str = "no_supported_node_commands";
    /// The selected Node package-manager binary is not on PATH.
    pub const MISSING_PACKAGE_MANAGER: &str = "missing_package_manager";
}

/// One actionable onboarding diagnostic.
///
/// Diagnostics are structured (never a substitute for a green baseline) and
/// appear in the detection, inspection, and init reports so tooling can
/// explain why a pack was resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitDiagnostic {
    /// Stable machine-readable code.
    pub code: String,
    /// Repository-relative path the diagnostic concerns.
    pub path: String,
    /// What was found.
    pub message: String,
    /// What the user can do about it.
    pub action: String,
}

impl InitDiagnostic {
    /// `package.json` exists but is not valid JSON.
    #[must_use]
    pub fn invalid_package_json(path: &str, error: &str) -> Self {
        Self {
            code: code::INVALID_PACKAGE_JSON.to_owned(),
            path: path.to_owned(),
            message: format!("package.json is not valid JSON: {error}"),
            action: "Fix the manifest syntax (or delete it) and re-run init; \
                     the generic pack was selected."
                .to_owned(),
        }
    }

    /// `package.json` parses but proves no supported Node command.
    #[must_use]
    pub fn no_supported_node_commands(path: &str) -> Self {
        Self {
            code: code::NO_SUPPORTED_NODE_COMMANDS.to_owned(),
            path: path.to_owned(),
            message: "package.json declares no supported Node commands (typecheck/lint/test/build \
                 scripts, turbo tasks, or matching dependencies)"
                .to_owned(),
            action: "Add a test, lint, typecheck, or build script (or matching dependency) and \
                     re-run init, or pass --language node explicitly."
                .to_owned(),
        }
    }

    /// The selected package-manager binary is not on PATH.
    #[must_use]
    pub fn missing_package_manager(package_manager: &str, path: &str) -> Self {
        Self {
            code: code::MISSING_PACKAGE_MANAGER.to_owned(),
            path: path.to_owned(),
            message: format!("the {package_manager} package manager was not found on PATH"),
            action: format!(
                "Install {package_manager} and re-run `do-harness init --force` to include \
                 the discovered Node sensors"
            ),
        }
    }
}

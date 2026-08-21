//! Version metadata and formatting.

use std::fmt;

use serde::Serialize;

use crate::report::Format;

/// Version and build metadata for `do-harness`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionInfo {
    /// Package name ("do-harness").
    pub name: &'static str,
    /// Cargo semver version.
    pub version: &'static str,
    /// Git commit short SHA if available at build time.
    pub commit: Option<&'static str>,
    /// Git commit date (YYYY-MM-DD) if available at build time.
    pub commit_date: Option<&'static str>,
    /// Whether the build tree had uncommitted changes.
    pub dirty: bool,
}

impl VersionInfo {
    /// Constructs [`VersionInfo`] from compile-time environment variables.
    #[must_use]
    pub const fn current() -> Self {
        let commit = match option_env!("DO_HARNESS_GIT_SHA") {
            Some(s) if !s.is_empty() => Some(s),
            _ => None,
        };
        let commit_date = match option_env!("DO_HARNESS_GIT_DATE") {
            Some(d) if !d.is_empty() => Some(d),
            _ => None,
        };
        let dirty = match option_env!("DO_HARNESS_GIT_DIRTY") {
            Some(v) => matches!(v.as_bytes(), b"true" | b"1"),
            None => false,
        };

        Self {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            commit,
            commit_date,
            dirty,
        }
    }

    /// Formats the version string suffix (excluding the binary name) for clap.
    #[must_use]
    pub fn version_suffix(&self) -> String {
        if let (Some(commit), Some(date)) = (self.commit, self.commit_date) {
            let dirty_suffix = if self.dirty { "-dirty" } else { "" };
            format!("{} ({commit}{dirty_suffix} {date})", self.version)
        } else {
            self.version.to_string()
        }
    }

    /// Formats version info for display according to the specified [`Format`].
    #[must_use]
    pub fn format(&self, format: Format) -> String {
        match format {
            Format::Text => self.to_string(),
            Format::Json => serde_json::to_string_pretty(self).unwrap_or_default(),
        }
    }
}

/// Returns the version suffix string for clap attribute usage.
#[must_use]
pub fn version_str() -> &'static str {
    Box::leak(VersionInfo::current().version_suffix().into_boxed_str())
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.name, self.version)?;
        if let (Some(commit), Some(date)) = (self.commit, self.commit_date) {
            let dirty_suffix = if self.dirty { "-dirty" } else { "" };
            write!(f, " ({commit}{dirty_suffix} {date})")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info_display_full() {
        let info = VersionInfo {
            name: "do-harness",
            version: "0.1.0",
            commit: Some("a1b2c3d"),
            commit_date: Some("2026-08-19"),
            dirty: false,
        };
        assert_eq!(info.to_string(), "do-harness 0.1.0 (a1b2c3d 2026-08-19)");
        assert_eq!(info.version_suffix(), "0.1.0 (a1b2c3d 2026-08-19)");
    }

    #[test]
    fn test_version_info_display_dirty() {
        let info = VersionInfo {
            name: "do-harness",
            version: "0.1.0",
            commit: Some("a1b2c3d"),
            commit_date: Some("2026-08-19"),
            dirty: true,
        };
        assert_eq!(
            info.to_string(),
            "do-harness 0.1.0 (a1b2c3d-dirty 2026-08-19)"
        );
        assert_eq!(info.version_suffix(), "0.1.0 (a1b2c3d-dirty 2026-08-19)");
    }

    #[test]
    fn test_version_info_display_no_git() {
        let info = VersionInfo {
            name: "do-harness",
            version: "0.1.0",
            commit: None,
            commit_date: None,
            dirty: false,
        };
        assert_eq!(info.to_string(), "do-harness 0.1.0");
        assert_eq!(info.version_suffix(), "0.1.0");
    }

    #[test]
    fn test_version_info_json() {
        let info = VersionInfo {
            name: "do-harness",
            version: "0.1.0",
            commit: Some("a1b2c3d"),
            commit_date: Some("2026-08-19"),
            dirty: false,
        };
        let json = info.format(Format::Json);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["name"], "do-harness");
        assert_eq!(value["version"], "0.1.0");
        assert_eq!(value["commit"], "a1b2c3d");
        assert_eq!(value["commit_date"], "2026-08-19");
        assert_eq!(value["dirty"], false);
    }

    #[test]
    fn test_version_info_json_no_git() {
        let info = VersionInfo {
            name: "do-harness",
            version: "0.1.0",
            commit: None,
            commit_date: None,
            dirty: false,
        };
        let json = info.format(Format::Json);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["name"], "do-harness");
        assert_eq!(value["version"], "0.1.0");
        assert!(value["commit"].is_null());
        assert!(value["commit_date"].is_null());
        assert_eq!(value["dirty"], false);
    }
}

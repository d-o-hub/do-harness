//! Workspace glob discovery and change-scope projection.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, anyhow};
use globset::GlobBuilder;

const PRUNED_DIRS: [&str; 8] = [
    ".git",
    ".do-harness",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
];

pub(crate) fn discover_workspaces(root: &Path, pattern: &str) -> anyhow::Result<Vec<PathBuf>> {
    let pattern_path = Path::new(pattern);
    if pattern.is_empty()
        || pattern_path.is_absolute()
        || pattern_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(anyhow!(
            "--workspaces requires a non-empty root-relative glob without '..'"
        ));
    }
    let mut builder = GlobBuilder::new(pattern);
    builder.literal_separator(true);
    builder.backslash_escape(false);
    let matcher = builder.build()?.compile_matcher();
    let mut pending = vec![root.to_path_buf()];
    let mut workspaces = Vec::new();

    while let Some(directory) = pending.pop() {
        let relative = relative_path(root, &directory)?;
        let config_path = directory.join("do-harness.toml");
        match fs::metadata(&config_path) {
            Ok(metadata) if metadata.is_file() => {
                let config_relative = if relative.is_empty() {
                    "do-harness.toml".to_owned()
                } else {
                    format!("{relative}/do-harness.toml")
                };
                if matcher.is_match(&relative) || matcher.is_match(&config_relative) {
                    workspaces.push(directory.clone());
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", config_path.display()));
            }
        }

        for entry in fs::read_dir(&directory)
            .with_context(|| format!("failed to read {}", directory.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            if name
                .to_str()
                .is_some_and(|name| PRUNED_DIRS.contains(&name))
            {
                continue;
            }
            pending.push(entry.path());
        }
    }

    workspaces.sort();
    workspaces.dedup();
    if workspaces.is_empty() {
        return Err(anyhow!(
            "workspace glob '{pattern}' matched no do-harness.toml files under {}",
            root.display()
        ));
    }
    Ok(workspaces)
}

pub(crate) fn relative_path(root: &Path, path: &Path) -> anyhow::Result<String> {
    let relative = path.strip_prefix(root).with_context(|| {
        format!(
            "{} is outside workspace root {}",
            path.display(),
            root.display()
        )
    })?;
    let mut normalized = String::new();
    for component in relative.components() {
        let component = component
            .as_os_str()
            .to_str()
            .ok_or_else(|| anyhow!("workspace path is not valid UTF-8: {}", path.display()))?;
        if !normalized.is_empty() {
            normalized.push('/');
        }
        normalized.push_str(component);
    }
    Ok(normalized)
}

pub(crate) fn workspace_change_scope(relative: &str, changed_files: &[String]) -> (bool, bool) {
    let mut affected = false;
    let mut run_unfiltered = false;
    for path in changed_files {
        if path == "do-harness.toml" {
            affected = true;
            run_unfiltered = true;
            continue;
        }
        if relative.is_empty() {
            affected = true;
            continue;
        }
        let config_path = format!("{relative}/do-harness.toml");
        if path == relative || path == &config_path {
            affected = true;
            run_unfiltered = true;
        } else if path
            .strip_prefix(relative)
            .is_some_and(|suffix| suffix.starts_with('/'))
        {
            affected = true;
        }
    }
    (affected, run_unfiltered)
}

pub(crate) fn is_workspace_relative(path: &Path) -> bool {
    !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{is_workspace_relative, workspace_change_scope};
    use std::path::Path;

    #[test]
    fn workspace_change_scope_uses_path_boundaries_and_policy_files() {
        let changed = vec![
            "packages/api/src/lib.rs".to_owned(),
            "packages/api/do-harness.toml".to_owned(),
            "packages/api-client/src/lib.rs".to_owned(),
        ];
        assert_eq!(
            workspace_change_scope("packages/api", &changed),
            (true, true)
        );
        assert_eq!(
            workspace_change_scope("packages/api-client", &changed),
            (true, false)
        );
        assert_eq!(
            workspace_change_scope("packages/other", &changed),
            (false, false)
        );
    }

    #[test]
    fn workspace_output_paths_cannot_escape_each_root() {
        assert!(is_workspace_relative(Path::new("evidence.json")));
        assert!(!is_workspace_relative(Path::new("../evidence.json")));
        assert!(!is_workspace_relative(Path::new("/tmp/evidence.json")));
    }
}

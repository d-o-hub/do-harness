//! Management of git hooks that run `do-harness verify` before commits and
//! pushes.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::hook_script::{
    BinSource, MARKER, commit_msg_body, only_args, resolve_binary, script_body,
};

/// Finds the `.git` directory (or git-dir) for the repository containing `cwd`.
///
/// Walks up from `cwd`, returning the first directory that contains an entry
/// named `.git` (a directory for a normal checkout, a file for a linked
/// worktree). When no walk-up match is found, falls back to running
/// `git rev-parse --git-dir` from `cwd`, which also supports bare
/// repositories and alternative layouts.
///
/// # Errors
///
/// Returns an error when neither method locates a repository.
pub fn find_git_dir(cwd: &Path) -> Result<PathBuf> {
    let mut current = Some(cwd);
    while let Some(dir) = current {
        let dot_git = dir.join(".git");
        if dot_git.exists() {
            return resolve_dot_git(&dot_git, dir);
        }
        current = dir.parent();
    }

    let output = crate::changes::git_command(cwd)
        .args(["rev-parse", "--git-common-dir"])
        .output();

    let git_common_str = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        _ => {
            let output_dir = crate::changes::git_command(cwd)
                .args(["rev-parse", "--git-dir"])
                .output()
                .with_context(|| format!("failed to run `git rev-parse` from {}", cwd.display()))?;
            if !output_dir.status.success() {
                bail!(
                    "no git repository found from {} (walk-up found no .git and `git rev-parse` failed)",
                    cwd.display()
                );
            }
            String::from_utf8_lossy(&output_dir.stdout)
                .trim()
                .to_owned()
        }
    };

    let path = PathBuf::from(&git_common_str);
    let abs_path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    Ok(fs::canonicalize(&abs_path).unwrap_or(abs_path))
}

fn resolve_dot_git(dot_git: &Path, dir: &Path) -> Result<PathBuf> {
    let target_git_dir = if dot_git.is_dir() {
        dot_git.to_path_buf()
    } else {
        let content = fs::read_to_string(dot_git)
            .with_context(|| format!("failed to read gitfile {}", dot_git.display()))?;
        let gitdir_line = content
            .lines()
            .find(|line| line.trim_start().starts_with("gitdir:"))
            .with_context(|| format!("invalid gitfile format at {}", dot_git.display()))?;
        let ptr = gitdir_line
            .trim_start()
            .strip_prefix("gitdir:")
            .ok_or_else(|| anyhow::anyhow!("invalid gitdir prefix at {}", dot_git.display()))?
            .trim();
        let ptr_path = Path::new(ptr);
        if ptr_path.is_absolute() {
            ptr_path.to_path_buf()
        } else {
            dir.join(ptr_path)
        }
    };

    let commondir_file = target_git_dir.join("commondir");
    let final_dir = if commondir_file.is_file() {
        if let Ok(commondir_content) = fs::read_to_string(&commondir_file) {
            let cptr = commondir_content.trim();
            if cptr.is_empty() {
                target_git_dir
            } else {
                let cpath = Path::new(cptr);
                if cpath.is_absolute() {
                    cpath.to_path_buf()
                } else {
                    target_git_dir.join(cpath)
                }
            }
        } else {
            target_git_dir
        }
    } else {
        target_git_dir
    };

    Ok(fs::canonicalize(&final_dir).unwrap_or(final_dir))
}

/// Queries Git configuration for `core.hooksPath`.
#[must_use]
pub fn get_core_hooks_path(repo_root: &Path) -> Option<String> {
    let output = crate::changes::git_command(repo_root)
        .args(["config", "--get", "core.hooksPath"])
        .output()
        .ok()?;
    if output.status.success() {
        let val = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !val.is_empty() {
            return Some(val);
        }
    }
    None
}

/// Computes the effective hooks directory where Git will execute hooks, along
/// with any raw `core.hooksPath` config string.
#[must_use]
pub fn get_effective_hooks_dir(git_dir: &Path, repo_root: &Path) -> (PathBuf, Option<String>) {
    let core_hooks_path = get_core_hooks_path(repo_root);
    if let Some(ref path_str) = core_hooks_path {
        let path = expand_tilde_and_resolve(path_str, repo_root, git_dir);
        (path, core_hooks_path)
    } else {
        (git_dir.join("hooks"), None)
    }
}

/// Expands leading `~` and resolves relative paths against `repo_root` (or `git_dir` if `.git` is a file).
fn expand_tilde_and_resolve(path_str: &str, repo_root: &Path, git_dir: &Path) -> PathBuf {
    let path_buf = if let Some(stripped) = path_str
        .strip_prefix("~/")
        .or_else(|| path_str.strip_prefix("~\\"))
    {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            PathBuf::from(home).join(stripped)
        } else {
            PathBuf::from(path_str)
        }
    } else {
        PathBuf::from(path_str)
    };

    if path_buf.is_absolute() {
        path_buf
    } else {
        let dot_git = repo_root.join(".git");
        if dot_git.is_file() {
            if let Ok(rel) = path_buf.strip_prefix(".git") {
                return git_dir.join(rel);
            }
        }
        repo_root.join(path_buf)
    }
}

/// Installs the pre-commit, pre-push, and commit-msg hooks into the active
/// hooks directory (`core.hooksPath` if configured, otherwise `git_dir/hooks/`).
///
/// `pre_commit` and `pre_push` are sensor name lists; an empty list means the
/// full suite (no `--only` flags). The `commit-msg` hook needs no sensor
/// arguments and runs the commitlint script against each prepared message. A
/// hook file that already exists without the [`MARKER`] is refused unless
/// `force` is `true`; files carrying the [`MARKER`] are always overwritten.
///
/// Returns the path where the hooks were installed.
///
/// # Errors
///
/// Returns an error when the target hooks directory does not exist, when a
/// foreign hook would be clobbered without `force`, or when a hook file cannot
/// be written.
pub fn install(
    git_dir: &Path,
    repo_root: &Path,
    pre_commit: &[String],
    pre_push: &[String],
    force: bool,
) -> Result<PathBuf> {
    let (hooks_dir, core_hooks_path) = get_effective_hooks_dir(git_dir, repo_root);
    if !hooks_dir.is_dir() {
        if let Some(cfg) = core_hooks_path {
            bail!(
                "hooks directory not found at {} (configured via core.hooksPath={}); create the directory or unset core.hooksPath with `git config --unset core.hooksPath`",
                hooks_dir.display(),
                cfg
            );
        }
        bail!(
            "hooks directory not found at {} (expected `git_dir/hooks`)",
            hooks_dir.display()
        );
    }

    write_hook_checked(
        &hooks_dir.join("pre-commit"),
        &script_body(&only_args(pre_commit)),
        force,
    )?;
    write_hook_checked(
        &hooks_dir.join("pre-push"),
        &script_body(&only_args(pre_push)),
        force,
    )?;
    write_hook_checked(&hooks_dir.join("commit-msg"), &commit_msg_body(), force)?;

    // If core.hooksPath points away from standard git_dir/hooks, clean up any
    // managed hooks in git_dir/hooks so they don't linger as shadowed artifacts.
    let default_hooks_dir = git_dir.join("hooks");
    if let (Ok(active), Ok(default)) = (
        fs::canonicalize(&hooks_dir),
        fs::canonicalize(&default_hooks_dir),
    ) {
        if active != default {
            for name in ["pre-commit", "pre-push", "commit-msg"] {
                let path = default_hooks_dir.join(name);
                if let Ok(content) = fs::read_to_string(&path) {
                    if content.contains(MARKER) {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }

    Ok(hooks_dir)
}

/// Removes managed hook files from both the effective hooks directory and
/// default `git_dir/hooks/`.
///
/// Foreign hook files are left untouched, and missing files or directories do
/// not produce an error.
///
/// # Errors
///
/// Returns an error when a managed hook file cannot be removed.
pub fn uninstall(git_dir: &Path, repo_root: &Path) -> Result<()> {
    let (hooks_dir, _) = get_effective_hooks_dir(git_dir, repo_root);
    let default_hooks_dir = git_dir.join("hooks");

    let mut dirs_to_clean = vec![hooks_dir];
    if default_hooks_dir != dirs_to_clean[0] {
        dirs_to_clean.push(default_hooks_dir);
    }

    for dir in dirs_to_clean {
        if !dir.is_dir() {
            continue;
        }
        for name in ["pre-commit", "pre-push", "commit-msg"] {
            let path = dir.join(name);
            if let Ok(content) = fs::read_to_string(&path) {
                if content.contains(MARKER) {
                    fs::remove_file(&path)
                        .with_context(|| format!("failed to remove {}", path.display()))?;
                }
            }
        }
    }
    Ok(())
}

/// Reports hook installation state.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookStatus {
    /// Whether the managed pre-commit hook file is present and active in `hooks_dir`.
    pub pre_commit: bool,
    /// Whether the managed pre-push hook file is present and active in `hooks_dir`.
    pub pre_push: bool,
    /// Whether the managed commit-msg hook file is present and active in `hooks_dir`.
    pub commit_msg: bool,
    /// Whether pre-commit is present in default `.git/hooks` but shadowed by `core.hooksPath`.
    pub pre_commit_shadowed: bool,
    /// Whether pre-push is present in default `.git/hooks` but shadowed by `core.hooksPath`.
    pub pre_push_shadowed: bool,
    /// Whether commit-msg is present in default `.git/hooks` but shadowed by `core.hooksPath`.
    pub commit_msg_shadowed: bool,
    /// Where the `do-harness` binary resolves from.
    pub binary: BinSource,
    /// Raw `core.hooksPath` git config value if set.
    pub core_hooks_path: Option<String>,
    /// Effective hooks directory where Git looks for hooks.
    pub hooks_dir: PathBuf,
}

impl HookStatus {
    /// Whether any managed hooks are shadowed by `core.hooksPath`.
    #[must_use]
    pub fn is_shadowed(&self) -> bool {
        self.pre_commit_shadowed || self.pre_push_shadowed || self.commit_msg_shadowed
    }

    /// Whether all managed hooks are installed and active in the effective hooks directory.
    #[must_use]
    pub fn is_all_installed(&self) -> bool {
        self.pre_commit && self.pre_push && self.commit_msg && !self.is_shadowed()
    }
}

/// Computes the installation state for `git_dir` given the repo root.
#[must_use]
pub fn status(git_dir: &Path, repo_root: &Path) -> HookStatus {
    let (hooks_dir, core_hooks_path) = get_effective_hooks_dir(git_dir, repo_root);
    let default_hooks_dir = git_dir.join("hooks");

    let is_custom_path = core_hooks_path.is_some() && hooks_dir != default_hooks_dir;

    let pre_commit = is_managed(&hooks_dir.join("pre-commit"));
    let pre_push = is_managed(&hooks_dir.join("pre-push"));
    let commit_msg = is_managed(&hooks_dir.join("commit-msg"));

    let pre_commit_shadowed =
        is_custom_path && !pre_commit && is_managed(&default_hooks_dir.join("pre-commit"));
    let pre_push_shadowed =
        is_custom_path && !pre_push && is_managed(&default_hooks_dir.join("pre-push"));
    let commit_msg_shadowed =
        is_custom_path && !commit_msg && is_managed(&default_hooks_dir.join("commit-msg"));

    HookStatus {
        pre_commit,
        pre_push,
        commit_msg,
        pre_commit_shadowed,
        pre_push_shadowed,
        commit_msg_shadowed,
        binary: resolve_binary(repo_root),
        core_hooks_path,
        hooks_dir,
    }
}

/// Returns whether the hook file at `path` exists and contains the [`MARKER`].
fn is_managed(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|content| content.contains(MARKER))
}

/// Writes `body` to `path`, refusing to clobber a foreign hook unless `force`
/// is set; hook files carrying the [`MARKER`] are always overwritten.
fn write_hook_checked(path: &Path, body: &str, force: bool) -> Result<()> {
    if let Ok(existing) = fs::read_to_string(path) {
        let managed = existing.contains(MARKER);
        if !managed && !force {
            bail!(
                "refusing to overwrite foreign hook {} (re-run with --force to replace it)",
                path.display()
            );
        }
    }
    write_hook(path, body)
}

/// Writes `body` to `path` and marks it executable.
fn write_hook(path: &Path, body: &str) -> Result<()> {
    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))?;
    crate::fs_perm::set_owner_exec(path)?;
    Ok(())
}

#[cfg(test)]
mod tests;

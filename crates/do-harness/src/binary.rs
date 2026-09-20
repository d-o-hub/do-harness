//! Binary resolution for `hook status` / `doctor`: where the `do-harness`
//! executable lives, probed from `$DO_HARNESS_BIN`, `PATH`, and the
//! repo-local Cargo fallback. Kept separate from `hook_script` so the
//! generated-script concerns and the host-side resolver stay under the
//! per-file size cap.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Where the `do-harness` binary resolves from for `hook status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinSource {
    /// `$DO_HARNESS_BIN`, verified executable.
    Env(PathBuf),
    /// Found on `PATH`.
    Path(PathBuf),
    /// The repo-local fallback path.
    Repo(PathBuf),
}

impl BinSource {
    /// Returns the underlying path for the binary source.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            BinSource::Env(p) | BinSource::Path(p) | BinSource::Repo(p) => p,
        }
    }

    /// Whether the resolved path currently exists on disk.
    #[must_use]
    pub fn present(&self) -> bool {
        match self {
            BinSource::Env(_) | BinSource::Path(_) => true,
            BinSource::Repo(path) => path.is_file(),
        }
    }

    /// Returns true if the resolved binary path lives inside a Cargo-managed `target` directory.
    #[must_use]
    pub fn is_in_target_dir(&self) -> bool {
        is_target_dir_path(self.path())
    }
}

/// Checks whether `path` (or its canonicalized path) contains a component named "target".
fn is_target_dir_path(path: &Path) -> bool {
    let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    p.components().any(|comp| comp.as_os_str() == "target")
}

/// Resolves the `do-harness` binary for `hook status` using the process
/// environment: `$DO_HARNESS_BIN`, then `PATH`, then the repo-local fallback.
#[must_use]
pub fn resolve_binary(repo_root: &Path) -> BinSource {
    resolve_binary_with(
        repo_root,
        std::env::var_os("DO_HARNESS_BIN").map(PathBuf::from),
        std::env::var_os("PATH"),
    )
}

/// Testable resolution core: `env_bin` and `path` stand in for the real
/// environment so lookups are deterministic.
fn resolve_binary_with(
    repo_root: &Path,
    env_bin: Option<PathBuf>,
    path: Option<OsString>,
) -> BinSource {
    resolve_binary_with_names(repo_root, env_bin, path, binary_names())
}

/// Resolution core with explicit probe names, so the Windows `.exe` fallback
/// stays testable on every platform.
fn resolve_binary_with_names(
    repo_root: &Path,
    env_bin: Option<PathBuf>,
    path: Option<OsString>,
    names: &[&str],
) -> BinSource {
    if let Some(bin) = env_bin
        && is_executable_file(&bin)
    {
        return BinSource::Env(bin);
    }
    if let Some(found) = search_path(path, names) {
        return BinSource::Path(found);
    }
    BinSource::Repo(repo_binary(repo_root, names))
}

/// File names the harness binary may carry, most preferred first; the last
/// entry is Cargo's `target/release` output name (`do-harness.exe` on
/// Windows, `do-harness` elsewhere). PATH probing mirrors the generated hook
/// script: the extensionless name wins when both exist.
#[must_use]
fn binary_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["do-harness", "do-harness.exe"]
    } else {
        &["do-harness"]
    }
}

/// Repo-local `target/release` fallback under the platform output name.
fn repo_binary(repo_root: &Path, names: &[&str]) -> PathBuf {
    let name = names.last().copied().unwrap_or("do-harness");
    repo_root.join("target/release").join(name)
}

/// Searches `PATH` entries for an executable harness binary under one of
/// `names` (first name wins within a directory).
fn search_path(path: Option<OsString>, names: &[&str]) -> Option<PathBuf> {
    let path = path?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// Whether `path` is a file and executable (unix); plain file otherwise.
fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn fake_repo() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        (temp, root)
    }

    #[cfg(unix)]
    fn write_exec(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(not(unix))]
    fn write_exec(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "stub").unwrap();
    }

    #[test]
    fn resolve_prefers_env_bin() {
        let (temp, root) = fake_repo();
        let bin = temp.path().join("envbin");
        write_exec(&bin);
        let source = resolve_binary_with(&root, Some(bin.clone()), None);
        assert_eq!(source, BinSource::Env(bin));
    }

    #[test]
    fn resolve_falls_back_to_path() {
        let (temp, root) = fake_repo();
        let bin = temp.path().join("bin").join("do-harness");
        write_exec(&bin);
        // PATH entries are separated by ';' on Windows and ':' elsewhere, and
        // the trailing POSIX-only entry must not be assumed elsewhere.
        let path = std::env::join_paths([
            temp.path().join("bin"),
            std::path::PathBuf::from("/usr/bin"),
        ])
        .unwrap();
        let source = resolve_binary_with(&root, None, Some(path));
        assert_eq!(source, BinSource::Path(bin));
    }

    #[test]
    fn resolve_uses_repo_fallback_when_nowhere() {
        let (_temp, root) = fake_repo();
        let source = resolve_binary_with(&root, None, Some(OsString::from("/nonexistent")));
        let name = if cfg!(windows) {
            "do-harness.exe"
        } else {
            "do-harness"
        };
        assert_eq!(
            source,
            BinSource::Repo(root.join("target/release").join(name))
        );
        assert!(!source.present());
    }

    #[test]
    fn resolve_finds_exe_fallback_on_path() {
        let (temp, root) = fake_repo();
        let bin = temp.path().join("bin").join("do-harness.exe");
        write_exec(&bin);
        let path = std::env::join_paths([temp.path().join("bin")]).unwrap();
        let names = ["do-harness", "do-harness.exe"];
        let source = resolve_binary_with_names(&root, None, Some(path), &names);
        assert_eq!(source, BinSource::Path(bin));
    }

    #[test]
    fn resolve_prefers_extensionless_over_exe() {
        let (temp, root) = fake_repo();
        let bin = temp.path().join("bin").join("do-harness");
        let exe = temp.path().join("bin").join("do-harness.exe");
        write_exec(&bin);
        write_exec(&exe);
        let path = std::env::join_paths([temp.path().join("bin")]).unwrap();
        let names = ["do-harness", "do-harness.exe"];
        let source = resolve_binary_with_names(&root, None, Some(path), &names);
        assert_eq!(source, BinSource::Path(bin));
    }

    #[test]
    fn resolve_repo_fallback_takes_last_probe_name() {
        let (_temp, root) = fake_repo();
        let names = ["do-harness", "do-harness.exe"];
        let source =
            resolve_binary_with_names(&root, None, Some(OsString::from("/nonexistent")), &names);
        assert_eq!(
            source,
            BinSource::Repo(root.join("target/release").join("do-harness.exe"))
        );
        assert!(!source.present());
    }

    #[test]
    #[cfg(windows)]
    fn binary_names_include_cargo_exe_on_windows() {
        assert_eq!(binary_names(), ["do-harness", "do-harness.exe"]);
    }

    #[test]
    #[cfg(not(windows))]
    fn binary_names_are_extensionless_off_windows() {
        assert_eq!(binary_names(), ["do-harness"]);
    }

    #[test]
    fn is_target_dir_path_detects_target_component() {
        assert!(is_target_dir_path(Path::new(
            "/home/user/project/target/release/do-harness"
        )));
        assert!(is_target_dir_path(Path::new("target/release/do-harness")));
        assert!(!is_target_dir_path(Path::new("/usr/local/bin/do-harness")));
        assert!(!is_target_dir_path(Path::new(
            "/home/user/.cargo/bin/do-harness"
        )));
    }

    #[test]
    fn bin_source_is_in_target_dir() {
        let bin = PathBuf::from("/repo/target/release/do-harness");
        assert!(BinSource::Repo(bin.clone()).is_in_target_dir());
        assert!(BinSource::Env(bin.clone()).is_in_target_dir());
        assert!(BinSource::Path(bin).is_in_target_dir());

        let cargo_bin = PathBuf::from("/home/user/.cargo/bin/do-harness");
        assert!(!BinSource::Path(cargo_bin).is_in_target_dir());
    }

    #[test]
    fn repo_source_is_present_when_file_exists() {
        let (temp, root) = fake_repo();
        let bin = root.join("target/release/do-harness");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, "stub").unwrap();
        assert!(BinSource::Repo(bin).present());
        assert!(temp.path().exists());
    }
}
